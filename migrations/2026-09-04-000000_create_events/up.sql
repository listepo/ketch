-- Every install, upgrade and uninstall ketch performs, appended and never
-- rewritten. One table answers both questions asked of it: filtered by package
-- and ordered by time it is that package's version history, and aggregated it
-- is the statistics.
--
-- Nothing here is authoritative. `state.json` remains the record of what is
-- installed; this is the record of what happened, and losing it costs history
-- rather than packages.
CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    package TEXT NOT NULL,
    -- `install`, `upgrade` or `uninstall`.
    action TEXT NOT NULL,
    version TEXT NOT NULL,
    -- The version this replaced. Null unless the action is an upgrade or a
    -- reinstall, which is what distinguishes the two.
    previous_version TEXT,
    tag TEXT NOT NULL,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    asset_name TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    checksum_verified BOOLEAN NOT NULL,
    -- Wall-clock time from the start of `prepare` to the end of `commit`:
    -- resolve, download, verify, unpack and place. Null for an uninstall,
    -- which does not go through that pipeline.
    --
    -- INTEGER rather than BIGINT so totalling it stays in integer arithmetic:
    -- Diesel folds a BIGINT sum into a decimal, which would mean carrying a
    -- bignum crate to add up milliseconds. The ceiling is 24 days per install.
    duration_ms INTEGER,
    at BIGINT NOT NULL,
    -- Which ketch wrote the row, so a statistic can be attributed to the
    -- version that produced it.
    ketch_version TEXT NOT NULL
);

-- The two orders anything reads this in: one package's history, and everything
-- newest first.
CREATE INDEX events_package_at ON events (package, at DESC);

CREATE INDEX events_at ON events (at DESC);
