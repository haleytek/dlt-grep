repo := "haleytek/dlt-grep"
gnu_bin := "dg-linux-x86_64-gnu"
musl_bin := "dg-linux-x86_64-musl"
# list available recipes
default:
    @just --list

# build Linux release binaries
build: build-gnu build-musl

# build the default Linux binary for glibc distributions
build-gnu:
    cargo build --release --target x86_64-unknown-linux-gnu
    cp target/x86_64-unknown-linux-gnu/release/dg {{gnu_bin}}
    @echo "built: {{gnu_bin}} ($(du -sh {{gnu_bin}} | cut -f1))"

# build a static Linux binary (requires musl-tools: sudo apt install musl-tools)
build-musl:
    cargo build --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/dg {{musl_bin}}
    @echo "built: {{musl_bin}} ($(du -sh {{musl_bin}} | cut -f1))"

# create a new release and upload the binary (requires: just build first)
release version: build
    git tag -f v{{version}}
    git push origin main
    git push --force origin v{{version}}
    GH_HOST=haleytek.ghe.com gh release create v{{version}} {{gnu_bin}} {{musl_bin}} \
        --repo {{repo}} \
        --title "v{{version}}"

# re-upload the binaries to an existing release (e.g. after a hotfix without a new tag)
upload version: build
    GH_HOST=haleytek.ghe.com gh release upload v{{version}} {{gnu_bin}} {{musl_bin}} \
        --repo {{repo}} \
        --clobber
