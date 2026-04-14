use crate::models::GlobalEvent;
use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::{Client, Collection, bson};
use tokio::sync::RwLock;

#[async_trait]
pub trait EventsRepo: Send + Sync {
    async fn fetch_events(
        &self,
        community_id: &str,
        payload_types: &[String],
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> anyhow::Result<Vec<GlobalEvent>>;
}

pub struct MongoEventsRepo {
    url: String,
    client: RwLock<Option<Client>>,
}

impl MongoEventsRepo {
    pub fn new(url: String) -> Self {
        Self {
            url,
            client: RwLock::new(None),
        }
    }

    async fn get_client(&self) -> anyhow::Result<Client> {
        if let Some(c) = self.client.read().await.clone() {
            return Ok(c);
        }
        let client = Client::with_uri_str(&self.url).await?;
        {
            let mut w = self.client.write().await;
            *w = Some(client.clone());
        }
        Ok(client)
    }

    async fn collection(&self) -> anyhow::Result<Collection<GlobalEvent>> {
        let client = self.get_client().await?;
        // Extract database name from URL
        let db_name = self
            .url
            .split('/')
            .nth(3)
            .and_then(|s| s.split('?').next())
            .unwrap_or("global-events");
        Ok(client.database(db_name).collection("events"))
    }
}

#[async_trait]
impl EventsRepo for MongoEventsRepo {
    async fn fetch_events(
        &self,
        community_id: &str,
        payload_types: &[String],
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
        limit: u32,
    ) -> anyhow::Result<Vec<GlobalEvent>> {
        let payload_types = payload_types
            .iter()
            .cloned()
            .map(bson::Bson::String)
            .collect::<Vec<_>>();

        let mut filter = bson::doc! {
            "communityId": community_id,
            "payloadType": { "$in": payload_types },
        };
        if let Some(f) = after {
            filter.insert(
                "createdAt",
                bson::doc! { "$gt": bson::DateTime::from_millis(f.timestamp_millis()) },
            );
        }
        if let Some(t) = before {
            match filter.get_mut("createdAt") {
                Some(bson::Bson::Document(d)) => {
                    d.insert("$lt", bson::DateTime::from_millis(t.timestamp_millis()));
                }
                _ => {
                    filter.insert(
                        "createdAt",
                        bson::doc! { "$lt": bson::DateTime::from_millis(t.timestamp_millis()) },
                    );
                }
            }
        }

        let coll = match self.collection().await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "Failed to get MongoDB collection");
                let mut w = self.client.write().await;
                *w = None;
                return Err(e);
            }
        };

        let mut cursor = match coll
            .find(filter)
            .sort(bson::doc! { "createdAt": 1 })
            .limit(limit as i64)
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(error = %e, "MongoDB find failed");
                let mut w = self.client.write().await;
                *w = None;
                return Err(e.into());
            }
        };

        let mut out = Vec::new();
        loop {
            match cursor.try_next().await {
                Ok(Some(event)) => {
                    out.push(event);
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::error!(error = %e, "MongoDB cursor iteration failed");
                    let mut w = self.client.write().await;
                    *w = None;
                    return Err(e.into());
                }
            }
        }
        Ok(out)
    }
}
