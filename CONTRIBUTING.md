## Build from source

If you do not have the Rust Toolchain, be sure to [install that first][rust site], and add ~/.cargo/bin to your shell's path.

If installation says you need to update your Rust version, [check here for instructions][install rust].

```shell
cargo install --force --git ssh://git@github.com/ybor-platform/ybor-cli.git ybor
```

## Releasing a new version

In order to release a new version for windows and the other operating systems, select the "Publish new version" job under the "Actions" tab. Select whether the release should be a major, minor, or patch release. This action will then compile the code on a number of operating systems and upload thems to [the Ybor Homebrew tap](https://github.com/ybor-tech/homebrew-tap) or [the official Microsoft winget repository](https://github.com/microsoft/winget-pkgs) as appropriate.

[rust site]: https://rustup.rs/
[install rust]: https://www.rust-lang.org/tools/install
