{ pkgs ? import <nixpkgs> {
    builtins = [(import (fetchTarball {
      url    = "https://github.com/NixOS/nixpkgs/archive/3389f23412877913b9d22a58dfb241684653d7e9.tar.gz";
      sha256 = "sha256:0wgm7sk9fca38a50hrsqwz6q79z35gqgb9nw80xz7pfdr4jy9pf8";
    }))];
    overlays = [
      (import (fetchGit {
        url = "https://github.com/oxalica/rust-overlay.git";
	rev = "fe185fac76e009b4bd543704a8c61077cf70155b";
      }))
    ];
   }
}:

pkgs.mkShell {
  name = "nomos-research-build-shell";

  buildInputs = with pkgs; [
    pkg-config
    rust-bin.stable.latest.minimal # 1.65.0
    go_1_19 # 1.19.3
  ];
}
