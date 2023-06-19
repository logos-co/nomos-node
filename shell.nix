{ pkgs ? import <nixpkgs> {
    builtins = [(import (fetchTarball {
      url    = "https://github.com/NixOS/nixpkgs/archive/e06c5e01088672bc460b2bc6b61d88e95190a492.tar.gz";
      sha256 = "sha256:e7d37547638aeb6b70a9dbf6dcc5970529edef39b46760a1c9689ac7f066ed58";
    }))];
    overlays = [
      (import (fetchGit {
        url = "https://github.com/oxalica/rust-overlay.git";
	rev = "3bab7ae4a80de02377005d611dc4b0a13082aa7c";
      }))
    ];
   }
}:

pkgs.mkShell {
  name = "nomos-research-build-shell";

  buildInputs = with pkgs; [
    pkg-config
    rust-bin.stable."1.70.0".default
    go_1_19 # 1.19.5
    clang_14
    llvmPackages_14.libclang
  ];
  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages_14.libclang.lib}/lib";
  '';
}
