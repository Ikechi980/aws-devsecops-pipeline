const express = require('express');
const crypto = require('crypto');

// --- Configuration ---------------------------------------------------------
const rawPort = process.env.PORT;
if (!rawPort) {
  throw new Error('PORT environment variable must be set for mock-yardi-api');
}
const port = Number.parseInt(rawPort, 10);
const serviceName = process.env.SERVICE_NAME || 'mock-yardi-api';

// --- State -----------------------------------------------------------------
const parents = new Map(); // apiKey -> { apiSecret, tokenTtlSeconds, organizations: Map }
const tokens = new Map(); // token -> { apiKey, expiresAt }
let failureConfig = createDefaultFailures();
let requestLog = []; // Track requests for testing

const EMPTY_BUNDLE = { resourceType: 'Bundle', entry: [] };

function createDefaultFailures() {
  return {
    tokenStatus: null,
    tokenBody: null,
    fhirStatus: null,
    fhirBody: null,
  };
}

function resetState() {
  parents.clear();
  tokens.clear();
  failureConfig = createDefaultFailures();
  requestLog = [];
}

resetState();

// --- HTTP Setup ------------------------------------------------------------
const app = express();
app.use(
  express.json({
    limit: '5mb',
  }),
);
app.use((err, _req, res, next) => {
  if (err instanceof SyntaxError) {
    return res.status(400).json({ error: 'Invalid JSON' });
  }
  return next(err);
});

// --- Health ----------------------------------------------------------------
app.get('/health', (_req, res) => {
  res.json({ status: 'ok', service: serviceName });
});

// --- OAuth Token Flow ------------------------------------------------------
app.post('/oauth/token', express.urlencoded({ extended: false }), (req, res) => {
  if (failureConfig.tokenStatus && failureConfig.tokenStatus !== 200) {
    return res
      .status(failureConfig.tokenStatus)
      .json(failureConfig.tokenBody || { error: 'Token failure' });
  }

  const credentials = parseBasicAuth(req.headers.authorization);
  if (!credentials) {
    return res.status(401).json({ error: 'Unauthorized' });
  }
  const parent = parents.get(credentials.apiKey);
  if (!parent || parent.apiSecret !== credentials.apiSecret) {
    return res.status(400).json({ error: 'Invalid credentials' });
  }

  requestLog.push({ type: 'token', apiKey: credentials.apiKey, timestamp: Date.now() });

  const token = crypto.randomUUID();
  const ttl = parent.tokenTtlSeconds || 300;
  tokens.set(token, { apiKey: credentials.apiKey, expiresAt: Date.now() + ttl * 1000 });

  return res.json({
    access_token: token,
    token_type: 'bearer',
    expires_in: ttl,
  });
});

function parseBasicAuth(header) {
  if (!header || typeof header !== 'string' || !header.toLowerCase().startsWith('basic ')) {
    return null;
  }
  try {
    const decoded = Buffer.from(header.slice(6), 'base64').toString('utf8');
    const [apiKey, apiSecret] = decoded.split(':');
    if (!apiKey || !apiSecret) {
      return null;
    }
    return { apiKey, apiSecret };
  } catch (_err) {
    return null;
  }
}

// --- FHIR Reads ------------------------------------------------------------
// Pattern: /Location or /Patient or /Encounter
app.get('/Location', (req, res) => {
  const orgId = req.query.organization || null;
  requestLog.push({ type: 'fhir', resource: 'Location', orgId, timestamp: Date.now() });
  if (!acceptsFhirJson(req, res)) return;
  if (shouldFailFhir(res)) return;

  const auth = authenticateToken(req.headers.authorization);
  if (!auth) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const orgEntry = resolveOrganization(auth.parent, orgId);
  if (!orgEntry) {
    return res.status(404).json({ error: 'Organization not found' });
  }

  return res
    .type('application/fhir+json')
    .json(paginateBundle(orgEntry.org.locations || EMPTY_BUNDLE, req));
});

app.get('/Patient', (req, res) => {
  const orgId = req.query.organization || null;
  requestLog.push({ type: 'fhir', resource: 'Patient', orgId, timestamp: Date.now() });
  if (!acceptsFhirJson(req, res)) return;
  if (shouldFailFhir(res)) return;

  const auth = authenticateToken(req.headers.authorization);
  if (!auth) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const orgEntry = resolveOrganization(auth.parent, orgId);
  if (!orgEntry) {
    return res.status(404).json({ error: 'Organization not found' });
  }

  return res
    .type('application/fhir+json')
    .json(paginateBundle(orgEntry.org.patients || EMPTY_BUNDLE, req));
});

app.get('/Patient/:id', (req, res) => {
  const orgId = req.query.organization || null;
  requestLog.push({
    type: 'fhir',
    resource: 'PatientDetail',
    orgId,
    patientId: req.params.id,
    timestamp: Date.now(),
  });
  if (!acceptsFhirJson(req, res)) return;
  if (shouldFailFhir(res)) return;

  const auth = authenticateToken(req.headers.authorization);
  if (!auth) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const organizationCandidates = orgId
    ? [auth.parent.organizations.get(orgId)].filter(Boolean)
    : Array.from(auth.parent.organizations.values());

  for (const org of organizationCandidates) {
    const entries = org?.patients?.entry || [];
    const patientEntry = entries.find((entry) => entry?.resource?.id === req.params.id);
    if (patientEntry?.resource) {
      return res.type('application/fhir+json').json(patientEntry.resource);
    }
  }

  return res.status(404).json({ error: 'Patient not found' });
});

app.get('/Encounter', (req, res) => {
  const orgId = req.query.organization || null;
  const patientFilter = parsePatientFilter(req.query.patient);
  requestLog.push({ type: 'fhir', resource: 'Encounter', orgId, timestamp: Date.now() });
  if (!acceptsFhirJson(req, res)) return;
  if (req.query.subject) {
    return res
      .status(400)
      .type('application/fhir+json')
      .json(operationOutcome('Unknown search parameter subject for resource Encounter.'));
  }
  if (shouldFailFhir(res)) return;

  const auth = authenticateToken(req.headers.authorization);
  if (!auth) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  // Encounters are returned across all orgs for this parent
  const orgEntry = resolveOrganization(auth.parent, orgId);
  if (!orgEntry) {
    return res.status(404).json({ error: 'Organization not found' });
  }

  const bundle = orgEntry.org.encounters || EMPTY_BUNDLE;
  if (!patientFilter) {
    return res.type('application/fhir+json').json(paginateBundle(bundle, req));
  }

  const filtered = {
    ...bundle,
    entry: (bundle.entry || []).filter((entry) => {
      const subject = entry?.resource?.subject?.reference;
      if (!subject || typeof subject !== 'string') {
        return false;
      }
      const id = subject.split('/').pop();
      return patientFilter.has(id);
    }),
  };

  return res.type('application/fhir+json').json(paginateBundle(filtered, req));
});

app.get('/Organization/:id', (req, res) => {
  requestLog.push({ type: 'fhir', resource: 'Organization', orgId: req.params.id, timestamp: Date.now() });
  if (!acceptsFhirJson(req, res)) return;
  if (shouldFailFhir(res)) return;

  const auth = authenticateToken(req.headers.authorization);
  if (!auth) {
    return res.status(401).json({ error: 'Unauthorized' });
  }

  const org = auth.parent.organizations.get(req.params.id);
  if (!org) {
    return res.status(404).json({ error: 'Organization not found' });
  }

  return res.type('application/fhir+json').json({
    resourceType: 'Organization',
    id: req.params.id,
    name: req.params.id,
  });
});

function resolveOrganization(parent, orgId) {
  if (!parent || !parent.organizations || parent.organizations.size === 0) {
    return null;
  }
  if (orgId) {
    const org = parent.organizations.get(orgId);
    if (!org) {
      return null;
    }
    return { orgId, org };
  }
  const [defaultOrgId, org] = parent.organizations.entries().next().value;
  return { orgId: defaultOrgId, org };
}

function shouldFailFhir(res) {
  if (failureConfig.fhirStatus && failureConfig.fhirStatus !== 200) {
    res.status(failureConfig.fhirStatus).json(failureConfig.fhirBody || { error: 'FHIR failure' });
    return true;
  }
  return false;
}

function acceptsFhirJson(req, res) {
  const accept = req.headers.accept;
  if (!accept || accept === '*/*') {
    res
      .status(406)
      .type('application/fhir+json')
      .json(
        operationOutcome(
          'Unsupported Accept header */*. Supported headers are: application/xml, application/fhir+xml, application/xml+fhir, text/json, application/json, application/fhir+json, or application/json+fhir',
        ),
      );
    return false;
  }
  return true;
}

function parsePatientFilter(patientParam) {
  if (!patientParam || typeof patientParam !== 'string') {
    return null;
  }
  const ids = patientParam
    .split(',')
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
  if (ids.length === 0) {
    return null;
  }
  return new Set(ids);
}

function paginateBundle(bundle, req) {
  const entries = Array.isArray(bundle.entry) ? bundle.entry : [];
  const rawPageSize = Number.parseInt(bundle.mockPageSize, 10);

  if (!Number.isFinite(rawPageSize) || rawPageSize <= 0 || entries.length <= rawPageSize) {
    const { mockPageSize, ...rest } = bundle;
    return rest;
  }

  const offset = parsePositiveInt(req.query._getpagesoffset, 0);
  const count = parsePositiveInt(req.query._count, rawPageSize);
  const pageEntries = entries.slice(offset, offset + count);
  const links = [];

  if (offset + count < entries.length) {
    const nextUrl = new URL(`${req.protocol}://${req.get('host')}${req.path}`);
    for (const [key, value] of Object.entries(req.query)) {
      if (key === '_getpagesoffset' || key === '_count') {
        continue;
      }
      if (Array.isArray(value)) {
        value.forEach((item) => nextUrl.searchParams.append(key, item));
      } else if (value !== undefined) {
        nextUrl.searchParams.set(key, value);
      }
    }
    nextUrl.searchParams.set('_getpagesoffset', String(offset + count));
    nextUrl.searchParams.set('_count', String(count));
    links.push({ relation: 'next', url: nextUrl.toString() });
  }

  const { mockPageSize, ...rest } = bundle;
  return {
    ...rest,
    entry: pageEntries,
    link: links,
  };
}

function parsePositiveInt(value, fallback) {
  const parsed = Number.parseInt(value, 10);
  if (Number.isFinite(parsed) && parsed >= 0) {
    return parsed;
  }
  return fallback;
}

function operationOutcome(diagnostics) {
  return {
    resourceType: 'OperationOutcome',
    issue: [
      {
        severity: 'error',
        code: 'processing',
        diagnostics,
      },
    ],
  };
}

function authenticateToken(header) {
  if (!header || typeof header !== 'string' || !header.toLowerCase().startsWith('bearer ')) {
    return null;
  }
  const token = header.slice(7);
  const record = tokens.get(token);
  if (!record || record.expiresAt < Date.now()) {
    tokens.delete(token);
    return null;
  }
  const parent = parents.get(record.apiKey);
  if (!parent) {
    return null;
  }
  return { parent };
}

app.use((req, res, next) => {
  if (req.path.startsWith('/admin/')) {
    return next();
  }
  if (req.path.startsWith('/')) {
    const resource = req.path.replace('/', '') || 'Unknown';
    return res
      .status(400)
      .type('application/fhir+json')
      .json(
        operationOutcome(
          `Invalid Resource specified: ${resource}. Valid Resources are: Patient, AllergyIntolerance, Binary, Condition, DocumentReference, Encounter, Immunization, Location, Medication, MedicationAdministration, MedicationRequest, MedicationStatement, NutritionOrder, Observation, Organization, Practitioner`,
        ),
      );
  }
  return res.status(404).json({ error: 'Not found' });
});

// --- Admin Helpers ---------------------------------------------------------
app.post('/admin/organizations', (req, res) => {
  const { apiKey, apiSecret, tokenTtlSeconds = 300, organizations } = req.body || {};
  if (!apiKey || !apiSecret || !Array.isArray(organizations) || organizations.length === 0) {
    return res.status(400).json({ error: 'Invalid organization payload' });
  }

  let parent = parents.get(apiKey);
  if (!parent) {
    parent = { apiSecret, tokenTtlSeconds, organizations: new Map() };
    parents.set(apiKey, parent);
  } else {
    parent.apiSecret = apiSecret;
    parent.tokenTtlSeconds = tokenTtlSeconds;
  }

  for (const organization of organizations) {
    if (!organization.organizationId) {
      return res.status(400).json({ error: 'Missing organizationId' });
    }
    parent.organizations.set(organization.organizationId, {
      patients: organization.patients || EMPTY_BUNDLE,
      encounters: organization.encounters || EMPTY_BUNDLE,
      locations: organization.locations || EMPTY_BUNDLE,
    });
  }

  return res.status(204).end();
});

app.put('/admin/organizations/:orgId', (req, res) => {
  // Update a specific organization's data
  const { apiKey, patients, encounters, locations } = req.body || {};
  if (!apiKey) {
    return res.status(400).json({ error: 'Missing apiKey' });
  }
  
  const parent = parents.get(apiKey);
  if (!parent) {
    return res.status(404).json({ error: 'Parent not found for apiKey' });
  }
  
  const org = parent.organizations.get(req.params.orgId);
  if (!org) {
    return res.status(404).json({ error: 'Organization not found' });
  }
  
  if (patients !== undefined) org.patients = patients;
  if (encounters !== undefined) org.encounters = encounters;
  if (locations !== undefined) org.locations = locations;
  
  return res.status(204).end();
});

app.post('/admin/invalidate', (req, res) => {
  const { token, apiKey } = req.body || {};
  if (token) {
    tokens.delete(token);
  } else if (apiKey) {
    for (const [storedToken, entry] of tokens.entries()) {
      if (entry.apiKey === apiKey) {
        tokens.delete(storedToken);
      }
    }
  }
  return res.status(204).end();
});

app.post('/admin/reset', (_req, res) => {
  resetState();
  res.status(204).end();
});

app.post('/admin/failures', (req, res) => {
  try {
    const { tokenStatus, tokenBody, fhirStatus, fhirBody } = req.body || {};
    if (tokenStatus !== undefined) {
      failureConfig.tokenStatus = sanitizeStatus(tokenStatus);
      failureConfig.tokenBody = tokenBody || null;
    }
    if (fhirStatus !== undefined) {
      failureConfig.fhirStatus = sanitizeStatus(fhirStatus);
      failureConfig.fhirBody = fhirBody || null;
    }
    return res.status(204).end();
  } catch (err) {
    return res.status(400).json({ error: err.message });
  }
});

app.get('/admin/requests', (req, res) => {
  // Return request log for testing
  const type = req.query.type;
  if (type) {
    return res.json(requestLog.filter(r => r.type === type));
  }
  return res.json(requestLog);
});

app.delete('/admin/requests', (_req, res) => {
  requestLog = [];
  return res.status(204).end();
});

function sanitizeStatus(value) {
  if (value === null || value === undefined) {
    return null;
  }
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed)) {
    throw new Error('Invalid status code');
  }
  return parsed;
}

// --- Fallback --------------------------------------------------------------
app.use((_req, res) => {
  res.status(404).json({ error: 'Not Found' });
});

app.listen(port, () => {
  console.log(`${serviceName} listening on port ${port}`);
});
