{ pkgs ? import <nixpkgs> {
    builtins = [(import (fetchTarball {
      url    = "https://github.com/NixOS/nixpkgs/archive/e06c5e01088672bc460b2bc6b61d88e95190a492.tar.gz";
      sha256 = "sha256:e7d37547638aeb6b70a9dbf6dcc5970529edef39b46760a1c9689ac7f066ed58";
    }))];
    overlays = [
      (import (fetchGit {
        url = "https://github.com/oxalica/rust-overlay.git";
        rev = "a0df72e106322b67e9c6e591fe870380bd0da0d5";
      }))
    ];
   }
}:

pkgs.mkShell {
  name = "nomos-research-build-shell";

  buildInputs = with pkgs; [
    pkg-config
    rust-bin.stable."1.75.0".default
    clang_14
    llvmPackages_14.libclang
    openssl
  ];
  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages_14.libclang.lib}/lib";
  '';
}
