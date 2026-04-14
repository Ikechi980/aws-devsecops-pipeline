const express = require('express');

const SYSTEMS = [
  {
    communityId: 'alpha',
    coreCommunityId: '11111111-1111-1111-1111-111111111111',
    networkIp: '10.0.0.5',
    localIp: '192.168.10.5',
    communityName: 'Alpha Community',
    fqdn: 'alpha.ensurelink.net',
  },
  {
    communityId: 'beta',
    coreCommunityId: '22222222-2222-2222-2222-222222222222',
    networkIp: '10.0.0.6',
    localIp: '192.168.10.6',
    communityName: 'Beta Community',
    fqdn: 'beta.ensurelink.net',
  },
  {
    communityId: 'gamma',
    coreCommunityId: null,
    networkIp: '10.0.0.7',
    localIp: '192.168.10.7',
    communityName: 'Gamma Community',
    fqdn: 'gamma.ensurelink.net',
  },
  {
    communityId: 'test-local',
    coreCommunityId: '33333333-3333-3333-3333-333333333333',
    networkIp: '127.0.0.1',
    localIp: '127.0.0.1',
    communityName: 'Test Local',
    fqdn: 'test-local.ensurelink.net',
  },
];

const COMMUNITY_MAP = SYSTEMS.reduce((acc, system) => {
  acc[system.communityId] = system.networkIp;
  return acc;
}, {});

const app = express();

app.get('/communities/:communityId', (req, res) => {
  const networkIp = COMMUNITY_MAP[req.params.communityId];
  if (!networkIp) {
    return res.status(404).json({ error: 'Community not found' });
  }

  const system = SYSTEMS.find((entry) => entry.communityId === req.params.communityId);
  res.json({ networkIp, coreCommunityId: system?.coreCommunityId ?? null });
});

app.get('/api/v1/ensure-systems', (_req, res) => {
  res.json(SYSTEMS);
});

app.get('/api/v1/ensure-systems/:communityId', (req, res) => {
  if (req.params.communityId === 'ems-error') {
    return res.status(500).json({ error: 'Internal server error' });
  }

  if (req.params.communityId === 'ems-invalid') {
    res.set('Content-Type', 'application/json');
    return res.status(200).send('not-json');
  }

  if (req.params.communityId === 'core-error') {
    return res.json({
      communityId: 'core-error',
      coreCommunityId: 'error-500',
    });
  }

  if (req.params.communityId === 'core-invalid') {
    return res.json({
      communityId: 'core-invalid',
      coreCommunityId: 'invalid-json',
    });
  }

  if (req.params.communityId === 'core-missing') {
    return res.json({
      communityId: 'core-missing',
      coreCommunityId: 'missing',
    });
  }

  const system = SYSTEMS.find((entry) => entry.communityId === req.params.communityId);
  if (!system) {
    return res.status(404).json({ error: 'Community not found' });
  }
  res.json(system);
});

const port = Number(process.env.PORT || 8081);
const host = '0.0.0.0';

app.listen(port, host, () => {
  console.log(`Mock Systems API listening on http://${host}:${port}`);
});
