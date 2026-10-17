-- The register is now loaded by the Worker itself from the GitHub mirror
-- (src/ownership-sync.js): releases carry the source file's SHA-256 so a
-- re-published IDX file is reloaded, and holdings keep every column the CLI has.
ALTER TABLE own_releases ADD COLUMN sha256 TEXT;
ALTER TABLE own_releases ADD COLUMN issuers INTEGER;
ALTER TABLE own_releases ADD COLUMN entities INTEGER;
ALTER TABLE own_holdings ADD COLUMN nationality TEXT;
ALTER TABLE own_holdings ADD COLUMN domicile TEXT;
ALTER TABLE own_holdings ADD COLUMN shares_scripless INTEGER;
ALTER TABLE own_holdings ADD COLUMN shares_scrip INTEGER;
