with import <nixpkgs> {};

mkShell {
  name = "presence-monitor-shell";
  buildInputs = [
    cargo rustc
    sqlite
    diesel-cli
  ];
}
