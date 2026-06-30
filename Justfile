repo := "haleytek/dlt-grep"
bin  := "dg-linux-x86_64"

# list available recipes
default:
    @just --list

# build a static Linux binary (requires musl-tools: sudo apt install musl-tools)
build:
    cargo build --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/dg {{bin}}
    @echo "built: {{bin}} ($(du -sh {{bin}} | cut -f1))"

# create a new release and upload the binary (requires: just build first)
release version: build
    git tag -f v{{version}}
    git push origin main
    git push --force origin v{{version}}
    GH_HOST=haleytek.ghe.com gh release create v{{version}} {{bin}} \
        --repo {{repo}} \
        --title "v{{version}}"

# re-upload the binary to an existing release (e.g. after a hotfix without a new tag)
upload version: build
    GH_HOST=haleytek.ghe.com gh release upload v{{version}} {{bin}} \
        --repo {{repo}} \
        --clobber
