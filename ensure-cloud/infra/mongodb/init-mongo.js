// Initialize Global Events database and seed sample

const mongoDatabase = process.env.MONGO_DATABASE;
const mongoUsername = process.env.MONGO_USERNAME;
const mongoPassword = process.env.MONGO_PASSWORD;

db = db.getSiblingDB(mongoDatabase);

db.createUser({
  user: mongoUsername,
  pwd: mongoPassword,
  roles: [{ role: "readWrite", db: mongoDatabase }],
});

db.events.insertOne(
  {
    _id: "507f1f77-bcf8-46cd-9994-39110507f1f7",
    communityId: "alpha",
    payloadType: "device-info",
    payloadSchemaVersion: 1,
    payload: {
      senderMac: "ec:1b:bd:98:cb:74",
      deviceType: "Move",
      hardwareVersion: 5,
      button1Presses: 1,
      button2Presses: 0,
      button3Presses: 0,
      lifetimeTxCount: 133240,
      majorVersion: 0,
      minorVersion: 74
    },
    createdAt: new Date("2025-01-21T15:52:39.382Z"),
    insertedAt: new Date("2025-01-21T15:52:39.421Z")
  }
);
