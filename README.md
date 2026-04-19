# TidySQL

A SQL linter with auto-fix support. Formatting is planned but not yet implemented.

## Install

Build from source:

```bash
cargo install --path crates/tidysql
```

## Usage

TidySQL reads from files, directories, globs, or stdin and can auto-fix issues.

### Check (Lint)

```bash
# Check a file
tidysql check query.sql

# Check multiple paths
tidysql check queries/ migrations/

# Check with a glob
tidysql check --glob "sql/**/*.sql"

# Check with auto-fix
tidysql check query.sql --fix

# Read from stdin
cat query.sql | tidysql check

# Resolve config for stdin as if it came from a file
cat query.sql | tidysql check --stdin-filename path/to/query.sql

# Override dialect
tidysql check query.sql --dialect bigquery

# Override lint levels
tidysql check query.sql -W keyword_case -D explicit_union -A disallow_names
```

### LSP Server

```bash
tidysql lsp
```

## Configuration

TidySQL resolves configuration per file by looking for the nearest `tidysql.toml`
in that file's directory or any parent directory. Parent configs are only inherited
through an explicit `extend`.

Create one in your project to configure defaults:

```toml
extend = "../tidysql.toml"

[files]
include = ["**/*.sql"]
extend_include = ["**/*.ddl"]
exclude = ["target/**", "vendor/**"]
extend_exclude = ["generated/**"]
respect_gitignore = true
force_exclude = false

[core]
dialect = "ansi"  # ansi, athena, bigquery, clickhouse, databricks, duckdb, mysql, postgres, redshift, snowflake, sparksql, sqlite, trino, tsql

[lints]
explicit_union = { level = "warn" }
disallow_names = { level = "warn", names = ["temp"], regexes = ["^_"] }
keyword_case = { level = "warn", policy = "upper" }
```

For discovered files, `include`/`exclude` globs are resolved relative to the
directory containing the config file. Files passed explicitly are still processed
even if excluded, unless `force_exclude = true`.

### Lint Levels

- `allow` - Disable the lint
- `warn` - Report as warning
- `error` / `deny` - Report as error
- `info` - Report as info
- `hint` - Report as hint

## Documentation

Lint rule documentation:

- `docs/reference/lints/index.md` (overview)
- `docs/reference/lints/*.md` (per‑lint details)

## License

Apache-2.0
