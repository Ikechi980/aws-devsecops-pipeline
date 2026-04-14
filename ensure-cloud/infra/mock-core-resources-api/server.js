const express = require('express');

const COMMUNITIES = [
  {
    id: '11111111-1111-1111-1111-111111111111',
    name: 'Alpha Community',
    yardi_org_id: null,
    yardi_api_key: null,
    yardi_api_secret: null,
    yardi_api_base_url: 'https://api.alpha.example.com/fhir',
    yardi_token_url: 'https://api.alpha.example.com/oauth/token',
  },
  {
    id: '22222222-2222-2222-2222-222222222222',
    name: 'Beta Community',
    yardi_org_id: null,
    yardi_api_key: null,
    yardi_api_secret: null,
    yardi_api_base_url: null,
    yardi_token_url: null,
  },
  {
    id: '33333333-3333-3333-3333-333333333333',
    name: 'Local Test Community',
    yardi_org_id: null,
    yardi_api_key: null,
    yardi_api_secret: null,
    yardi_api_base_url: null,
    yardi_token_url: null,
  },
];

const LOCATIONS = [
  {
    id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
    community_id: '11111111-1111-1111-1111-111111111111',
    name: 'Alpha A Wing',
    location_type: 'apartment',
    yardi_reference_id: null,
  },
  {
    id: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
    community_id: '11111111-1111-1111-1111-111111111111',
    name: 'Alpha B Wing',
    location_type: 'apartment',
    yardi_reference_id: null,
  },
  {
    id: 'cccccccc-cccc-cccc-cccc-cccccccccccc',
    community_id: '22222222-2222-2222-2222-222222222222',
    name: 'Beta Floor 1',
    location_type: 'apartment',
    yardi_reference_id: null,
  },
];

const RESIDENTS = [
  {
    id: 'dddddddd-dddd-dddd-dddd-dddddddddddd',
    community_id: '11111111-1111-1111-1111-111111111111',
    location_id: 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa',
    first_name: 'Alice',
    last_name: 'Alpha',
    yardi_reference_id: null,
    photo: {
      etag: 'sha256:mock-alpha-resident-photo',
      content_type: 'image/png',
      size_bytes: 9,
      updated_at: '2026-01-01T00:00:00Z',
    },
  },
  {
    id: 'eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee',
    community_id: '11111111-1111-1111-1111-111111111111',
    location_id: 'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb',
    first_name: 'Aaron',
    last_name: 'Alpha',
    yardi_reference_id: null,
    photo: null,
  },
  {
    id: 'ffffffff-ffff-ffff-ffff-ffffffffffff',
    community_id: '22222222-2222-2222-2222-222222222222',
    location_id: 'cccccccc-cccc-cccc-cccc-cccccccccccc',
    first_name: 'Bella',
    last_name: 'Beta',
    yardi_reference_id: null,
    photo: null,
  },
];

const RESIDENT_PHOTOS = {
  'dddddddd-dddd-dddd-dddd-dddddddddddd': {
    bytes: Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5]),
    contentType: 'image/png',
    etag: '"sha256:mock-alpha-resident-photo"',
    lastModified: 'Thu, 01 Jan 2026 00:00:00 GMT',
  },
};

const app = express();

app.get('/v1/health', (_req, res) => {
  res.json({ status: 'ok' });
});

app.get('/v1/communities/:id', (req, res) => {
  if (req.params.id === 'error-500') {
    return res.status(500).json({ error: 'Internal server error' });
  }

  if (req.params.id === 'invalid-json') {
    res.set('Content-Type', 'application/json');
    return res.status(200).send('not-json');
  }

  const community = COMMUNITIES.find((entry) => entry.id === req.params.id);
  if (!community) {
    return res.status(404).json({ error: 'Community not found' });
  }
  res.json(community);
});

app.get('/v1/communities/:id/locations', (req, res) => {
  if (req.params.id === 'error-500') {
    return res.status(500).json({ error: 'Internal server error' });
  }

  if (req.params.id === 'invalid-json') {
    res.set('Content-Type', 'application/json');
    return res.status(200).send('not-json');
  }

  const community = COMMUNITIES.find((entry) => entry.id === req.params.id);
  if (!community) {
    return res.status(404).json({ error: 'Community not found' });
  }
  const locations = LOCATIONS.filter((entry) => entry.community_id === req.params.id);
  res.json(locations);
});

app.get('/v1/communities/:id/residents', (req, res) => {
  if (req.params.id === 'error-500') {
    return res.status(500).json({ error: 'Internal server error' });
  }

  if (req.params.id === 'invalid-json') {
    res.set('Content-Type', 'application/json');
    return res.status(200).send('not-json');
  }

  const community = COMMUNITIES.find((entry) => entry.id === req.params.id);
  if (!community) {
    return res.status(404).json({ error: 'Community not found' });
  }
  const residents = RESIDENTS.filter((entry) => entry.community_id === req.params.id);
  res.json(residents);
});

app.get('/v1/communities/:id/residents/:residentId/photo', (req, res) => {
  if (req.params.id === 'error-500') {
    return res.status(500).json({ error: 'Internal server error' });
  }

  const community = COMMUNITIES.find((entry) => entry.id === req.params.id);
  if (!community) {
    return res.status(404).json({ error: 'Community not found' });
  }

  const resident = RESIDENTS.find(
    (entry) =>
      entry.id === req.params.residentId && entry.community_id === req.params.id,
  );
  if (!resident) {
    return res.status(404).json({ error: 'Resident not found' });
  }

  const photo = RESIDENT_PHOTOS[req.params.residentId];
  if (!photo) {
    return res.status(404).json({ error: 'Resident photo not found' });
  }

  if (req.header('If-None-Match') === photo.etag) {
    res.set('ETag', photo.etag);
    return res.status(304).send();
  }

  res.set('Content-Type', photo.contentType);
  res.set('ETag', photo.etag);
  res.set('Last-Modified', photo.lastModified);
  return res.status(200).send(photo.bytes);
});

const port = Number(process.env.PORT || 8082);
const host = '0.0.0.0';

app.listen(port, host, () => {
  console.log(`Mock Core Resources API listening on http://${host}:${port}`);
});
