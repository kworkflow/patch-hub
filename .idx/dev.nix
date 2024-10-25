# To learn more about how to use Nix to configure your environment
# see: https://developers.google.com/idx/guides/customize-idx-env
{ pkgs, ... }: {
  channel = "stable-23.11";
  # Use https://search.nixos.org/packages to find packages
  packages = with pkgs; [
    gitFull
    rustup
    b4
    bat
    delta
    diff-so-fancy
    gcc
  ];

  # Sets environment variables in the workspace
  env = {};
  idx = {
    # Search for the extensions you want on https://open-vsx.org/ and use "publisher.id"
    extensions = [
      "franneck94.vscode-rust-extension-pack"
    ];

    previews.enable = false;

    workspace = {
      onCreate = {
        rust = "rustup default stable";
        first-build = "cargo build";
      };
    };
  };
}
