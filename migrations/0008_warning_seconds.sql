-- GC-08: ADR-001 exige avisos configuráveis por regra.
-- `warning_seconds` armazena até dois limiares separados por vírgula (ex.: "300,60").
-- NULL = usa o padrão do motor (300, 60).
ALTER TABLE active_rules ADD COLUMN warning_seconds TEXT
    CHECK (warning_seconds IS NULL
           OR warning_seconds GLOB '[0-9]*,[0-9]*'
           OR warning_seconds GLOB '[0-9]*');