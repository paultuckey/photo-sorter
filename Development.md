
# Development

## Rust code

Let's use the most basic rust we can to make the code as approachable as possible.

- Don't use lifetimes
- Don't use `unsafe` or `expect()`
- Only use `unwrap()` in tests
- Use `.clone()` to avoid hard things


```shell
cargo clippy
```

```shell
cargo run
```

```shell
cargo test
```

```shell
cargo build
```

Verify help is working:

```shell
cargo run -- --help
```

Test a single photo file:

```shell
cargo run -- markdown --debug --root "test" --input "Canon_40D.jpg"
```

Test a single album file:

```shell
cargo run -- markdown --debug --root "test/takeout1" --input "Google Photos/album1/metadata.json"
```

Dry run a sync operation:

```shell
cargo run -- \
  sync --debug --dry-run \
    --input "input/takeout-20250614T030613Z-1-001.zip" \
    --output "output/archive"
```

Sync a directory:

```shell
cargo run -- sync --debug --input "input/Takeout-small" --output "output/archive-small"
```


## Output

Console output is based on rsync. 

```sh
rsync --dry-run -a --verbose ../input/takeout-small/ ../output/takeout-small/
```

## Zip File Debugging


```sh
zipinfo -m input/takeout-20250614T030613Z-1-001.zip > output/takeout-list.txt
```