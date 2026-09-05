{
  description = "Awaz - local-first voice I/O development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
    in {
      devShells = forAllSystems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustup
              pkg-config
              cmake
              curl
              gnumake
            ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              alsa-lib
              pipewire
            ];

            shellHook = ''
              export RUSTUP_TOOLCHAIN=1.98.0
              if [ -d "$PWD/vendor/moonshine/lib" ]; then
                export AWAZ_MOONSHINE_LIB_DIR="$PWD/vendor/moonshine/lib"
                export LD_LIBRARY_PATH="$PWD/vendor/moonshine/lib''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
              fi
              echo "Awaz dev shell. Run ./scripts/dev-setup.sh once, then cargo test/build."
            '';
          };
        });
    };
}
