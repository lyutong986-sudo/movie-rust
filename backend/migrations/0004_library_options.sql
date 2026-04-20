ALTER TABLE libraries
    ADD COLUMN IF NOT EXISTS library_options jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS date_modified timestamptz NOT NULL DEFAULT now();

UPDATE libraries
SET library_options = jsonb_build_object(
        'Enabled', true,
        'EnablePhotos', true,
        'EnableRealtimeMonitor', false,
        'EnableAutomaticSeriesGrouping', true,
        'PreferredMetadataLanguage', 'zh',
        'MetadataCountryCode', 'CN',
        'SeasonZeroDisplayName', 'Specials',
        'MetadataSavers', jsonb_build_array('Nfo'),
        'LocalMetadataReaderOrder', jsonb_build_array('Nfo'),
        'PathInfos', jsonb_build_array(jsonb_build_object('Path', path))
    ),
    date_modified = now()
WHERE library_options = '{}'::jsonb;
