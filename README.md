# TidySQL

A SQL linter and formatter with auto-fix support.

## Usage

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

Create a `tidysql.toml` file:

```toml
[core]
dialect = "ansi"  # ansi, bigquery, clickhouse, databricks, mysql, redshift, snowflake, trino

[lints]
explicit_union = { level = "warn" }
disallow_names = { level = "warn", names = ["temp"], regexes = ["^_"] }
keyword_case = { level = "warn", policy = "upper" }
```

### Lint Levels

- `allow` - Disable the lint
- `warn` - Report as warning
- `error` / `deny` - Report as error

## Lint Rules

### `keyword_case`

Enforces consistent capitalisation of SQL keywords.

**Anti-pattern:**

```sql
SeLeCt 1 from blah
```

**Best practice:**

```sql
SELECT 1 FROM blah
-- or
select 1 from blah
```

**Configuration:**

```toml
[lints]
keyword_case = { level = "warn", policy = "upper" }
```

**Options:**

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `policy` | string | `"consistent"` | One of: `consistent`, `upper`, `lower`, `capitalise`, `pascal`, `snake`, `camel` |
| `ignore_words` | array | `[]` | Keywords to ignore (case-insensitive) |
| `ignore_words_regex` | array | `[]` | Regex patterns for keywords to ignore |

**Policies:**

- `consistent` - Infer from existing keywords (uppercase if majority are upper, otherwise lowercase)
- `upper` - `SELECT`, `FROM`, `WHERE`
- `lower` - `select`, `from`, `where`
- `capitalise` / `pascal` - `Select`, `From`, `Where`
- `snake` / `camel` - Same as `lower`

### `explicit_union`

Requires `UNION` statements to explicitly specify `ALL` or `DISTINCT`.

**Anti-pattern:**

```sql
SELECT 1 UNION SELECT 2
```

**Best practice:**

```sql
SELECT 1 UNION DISTINCT SELECT 2
-- or
SELECT 1 UNION ALL SELECT 2
```

### `disallow_names`

Disallows specific identifier names.

**Configuration:**

```toml
[lints]
disallow_names = { level = "warn", names = ["temp", "tmp"], regexes = ["^_"] }
```

## License

Apache-2.0
