{
  description = "SOLAR development flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    go-overlay = {
      url = "github:purpleclay/go-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    gomod2nix = {
      url = "github:nix-community/gomod2nix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = { nixpkgs, flake-utils, go-overlay, git-hooks, gomod2nix, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            go-overlay.overlays.default
            gomod2nix.overlays.default
          ];
        };
        lib = pkgs.lib;
        goVersion = "1.26.3";
      in
      {
        devShells.default = pkgs.mkShell {
          packages =  with pkgs; [
            curl
            gnumake
            jq
            kind
            kubectl
            kubernetes-helm
            shellcheck
            yq-go
            go-bin.versions.${goVersion}
            gotools
            nodejs_22
            pnpm
            chromium
          ];

          env.PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
          env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH = "${pkgs.chromium}/bin/chromium";
        };
      }
    );
}
