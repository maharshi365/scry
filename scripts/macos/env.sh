# Source from macOS launchers: select rustup proxies, never standalone Homebrew Rust.
export PATH="$HOME/.cargo/bin:$PATH"
if [[ ! -x "$HOME/.cargo/bin/rustc" ]]; then
    for prefix in /opt/homebrew /usr/local; do
        if [[ -x "$prefix/opt/rustup/bin/rustc" ]]; then
            export PATH="$prefix/opt/rustup/bin:$PATH"
            break
        fi
    done
fi
