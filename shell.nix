{ pkgs ? import <nixpkgs> {
    overlays = [
      (import (fetchTarball "https://github.com/oxalica/rust-overlay/archive/master.tar.gz"))
    ];
  }
}:

pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    pkg-config
    rust-bin.stable.latest.minimal
    go_1_19
  ];
}
