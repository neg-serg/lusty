{
  description = "lusty: native file/buffer picker for Neovim (serve backend + standalone TUI)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system} = {
        default = pkgs.callPackage ./default.nix { };
        lusty = self.packages.${system}.default;
      };
    };
}
