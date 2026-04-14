-- Create communities table
CREATE TABLE communities (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    yardi_org_id TEXT,
    yardi_api_key TEXT,
    yardi_api_secret TEXT,
    yardi_api_base_url TEXT,
    yardi_token_url TEXT,
    CONSTRAINT yardi_fields_all_or_none CHECK (
        (
            yardi_org_id IS NULL AND
            yardi_api_key IS NULL AND
            yardi_api_secret IS NULL AND
            yardi_api_base_url IS NULL AND
            yardi_token_url IS NULL
        ) OR
        (
            yardi_org_id IS NOT NULL AND
            yardi_api_key IS NOT NULL AND
            yardi_api_secret IS NOT NULL AND
            yardi_api_base_url IS NOT NULL AND
            yardi_token_url IS NOT NULL
        )
    )
);

-- Create locations table
CREATE TABLE locations (
    id UUID PRIMARY KEY,
    community_id UUID NOT NULL REFERENCES communities(id) ON DELETE RESTRICT,
    name TEXT NOT NULL,
    location_type TEXT NOT NULL,
    yardi_reference_id TEXT,
    CONSTRAINT locations_id_community_id_unique UNIQUE (id, community_id)
);

-- Create unique index for yardi_reference_id within a community (when set)
CREATE UNIQUE INDEX idx_locations_yardi_reference_id 
    ON locations(community_id, yardi_reference_id) 
    WHERE yardi_reference_id IS NOT NULL;

-- Create residents table with composite FK to ensure community consistency
CREATE TABLE residents (
    id UUID PRIMARY KEY,
    location_id UUID NOT NULL,
    community_id UUID NOT NULL,
    first_name TEXT NOT NULL,
    last_name TEXT NOT NULL,
    yardi_reference_id TEXT,
    CONSTRAINT residents_location_community_fk 
        FOREIGN KEY (location_id, community_id) 
        REFERENCES locations(id, community_id) ON DELETE RESTRICT
);

-- Create unique index for yardi_reference_id within a community (when set)
CREATE UNIQUE INDEX idx_residents_yardi_reference_id 
    ON residents(community_id, yardi_reference_id) 
    WHERE yardi_reference_id IS NOT NULL;

-- Create indexes for foreign keys
CREATE INDEX idx_locations_community_id ON locations(community_id);
CREATE INDEX idx_residents_location_id ON residents(location_id);
CREATE INDEX idx_residents_community_id ON residents(community_id);

-- Create resident photos table (0..1 photo per resident)
CREATE TABLE resident_photos (
    resident_id UUID PRIMARY KEY REFERENCES residents(id) ON DELETE CASCADE,
    content_type TEXT NOT NULL,
    image_data BYTEA NOT NULL,
    sha256 TEXT NOT NULL,
    source_last_updated TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    size_bytes INT GENERATED ALWAYS AS (octet_length(image_data)) STORED,
    CONSTRAINT resident_photos_content_type_valid CHECK (
        content_type IN ('image/jpeg', 'image/png', 'image/webp')
    ),
    CONSTRAINT resident_photos_sha256_valid CHECK (
        sha256 ~ '^[a-f0-9]{64}$'
    ),
    CONSTRAINT resident_photos_size_valid CHECK (
        size_bytes > 0 AND size_bytes <= 2097152
    )
);
