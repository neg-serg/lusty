{ lib, rustPlatform }:

rustPlatform.buildRustPackage rec {
  pname = "lusty";
  version = "0.1.0";

  src = ./.;

  cargoLock.lockFile = ./Cargo.lock;
  # cargoHash = ""; # filled after the first nix build reports the hash

  meta = with lib; {
    description = "Native file/buffer picker for Neovim (Lusty successor)";
    homepage = "https://github.com/neg-serg/lusty";
    license = licenses.mit;
    mainProgram = "lusty";
    platforms = platforms.linux;
    maintainers = [ ];
  };
}
