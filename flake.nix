{
  description = "A terminal-based EPUB/PDF Books reader";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        manifest = (pkgs.lib.importTOML ./Cargo.toml).package;
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = manifest.name;
          inherit (manifest) version;

          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
            python3
            rustPlatform.bindgenHook
            unzip
          ];

          buildInputs = with pkgs; [
            mupdf
            freetype
            harfbuzz
            openjpeg
            jbig2dec
            gumbo
            zlib
            fontconfig
          ];

          # Can't be tested correctly in sandbox environment
          checkFlags = [
            "--skip=test_definition_list_with_complex_content_svg"
            "--skip=test_mouse_scroll_file_list_svg"
            "--skip=test_toc_chapter_navigation_svg"
          ];

          meta = with pkgs.lib; {
            inherit (manifest) homepage description;
            license = licenses.agpl3Plus;
            mainProgram = "bookokrat";
          };
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
          packages = with pkgs; [
            rust-analyzer
            clippy
            rustfmt
          ];

          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };
      }
    );
}
