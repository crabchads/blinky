#!/usr/bin/env brush

reindeer --third-party-dir=third-party/rust buckify || exit 1
ln -sf $(buck2 bxl prelude//cxx/tools/compilation_database.bxl:generate -- --targets ...) ./compile_commands.json || exit1
rust-project develop --prefer-rustup-managed-toolchain //... || exit 1
# buck2 build //...
