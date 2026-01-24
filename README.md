# TidySQL

A SQL linter and formatter with auto-fix support.

## Install

Build from source:

```bash
cargo install --path crates/tidysql
```

## Usage

TidySQL reads from files or stdin and can auto-fix issues.

### Check (Lint)

```bash
# Check a file
tidysql check query.sql

# Check with auto-fix
tidysql check query.sql --fix

# Read from stdin
cat query.sql | tidysql check

# Override dialect
tidysql check query.sql --dialect bigquery

# Override lint levels
tidysql check query.sql -W keyword_case -D explicit_union -A disallow_names
```

### Format

```bash
# Format a file (prints to stdout)
tidysql format query.sql

# Format from stdin
cat query.sql | tidysql format
```

### LSP Server

```bash
tidysql lsp
```

## Configuration

TidySQL looks for `tidysql.toml` in the current directory or any parent directory.
Create one in your project to configure defaults:

```toml
[core]
dialect = "ansi"  # ansi, athena, bigquery, clickhouse, databricks, duckdb, mysql, postgres, redshift, snowflake, sparksql, sqlite, trino, tsql

[lints]
explicit_union = { level = "warn" }
disallow_names = { level = "warn", names = ["temp"], regexes = ["^_"] }
keyword_case = { level = "warn", policy = "upper" }
```

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
