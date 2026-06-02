# TidySQL

A SQL linter with auto-fix support and SELECT-first formatting.

## Install

Build from source:

```bash
cargo install --path crates/tidysql
```

## Usage

TidySQL reads from files, directories, globs, or stdin and can auto-fix issues.

TidySQL keeps formatting and linting separate:

- `tidysql format` rewrites visual style: whitespace, indentation, line breaks, comma layout, and keyword casing.
- `tidysql check` reports correctness, ambiguity, determinism, maintainability, convention, and project-policy diagnostics.
- `tidysql check --fix` applies lint fixes only. It does not run the formatter.

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
tidysql check query.sql -D explicit_union -A disallow_names
```

### Format

```bash
# Format a file to stdout
tidysql format query.sql

# Rewrite files in place
tidysql format queries/ --write

# Check whether files are already formatted
tidysql format queries/ --check

# Fail if any unsupported statement is encountered
tidysql format queries/ --check --strict

# Read from stdin
cat query.sql | tidysql format
```

### Recommended Workflow

While typing, editor integrations use a quiet diagnostic profile. They show parse
and lex errors plus high-signal lints such as NULL comparisons, constant
expressions, duplicate aliases, consecutive semicolons, and non-deterministic
LIMIT queries. They hide visual style and lower-signal convention or drafting
noise such as keyword casing, wrapping, COUNT style, not-equal operator style,
unused CTEs, and redundant ELSE NULL branches.

On save or on demand, run the formatter:

```bash
tidysql format query.sql --write
```

When you want intentional source rewrites, run lint fixes:

```bash
tidysql check query.sql --fix
```

In CI, keep the failures separate:

```bash
tidysql format . --check
tidysql check .
```

Formatting failures mean files need visual formatting. Lint failures mean queries
have correctness, determinism, maintainability, convention, or project-policy
issues.

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

[format]
line_width = 100
indent_width = 4
keyword_case = "upper"  # upper, lower, preserve
comma_style = "trailing"  # trailing, leading

[diagnostics]
profile = "quiet"  # quiet, recommended, strict

[lints]
explicit_union = { level = "warn" }
disallow_names = { level = "warn", names = ["temp"], regexes = ["^_"] }
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
