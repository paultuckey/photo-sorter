
# Development

## Rust code

Let's use the most basic rust we can to make the code as approachable as possible.

- Don't use lifetimes
- Don't use `unsafe` or `expect()`
- Only use `unwrap()` in tests
- Use `.clone()` to avoid hard things
- Don't use `async`/`await` (this type of I/O heavy work may not benefit that much)


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
cargo run -- info --debug --root "test" --input "Canon_40D.jpg"
```

Test a single album file:

```shell
cargo run -- info --debug --root "test/takeout1" --input "Google Photos/album1/metadata.json"
```

Check the index:

```shell
cargo run -- index --debug --input "input/Takeout"
```

```shell
cargo run -- index --debug --input "input/iCloud Photos"
```

Make a database:

```shell
cargo run -- db --debug --input "input/Takeout"
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

```shell
cargo run -- sync --input "input/takeout" --output "output/archive"
```

```shell
cargo run -- sync --input "input/icloud photos" --output "output/archive"
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

## Exif debugging

```shell
brew install exiftool
```

```shell
exiftool "input/iCloud Photos/Photos/IMG_5071.HEIC"
```

```shell
exiftool "input/Takeout/Google Photos/Photos from 2024/IMG_3986.HEIC" > a.txt
exiftool "input/iCloud Photos/Photos/IMG_3986.HEIC" > b.txt
```

## Notes

The same photo from different sources give different sizes:

```shell
ls -la "input/iCloud Photos/Photos/IMG_5071.HEIC"
ls -la "input/Takeout/Google Photos/Photos from 2025/IMG_5071.HEIC"
```
