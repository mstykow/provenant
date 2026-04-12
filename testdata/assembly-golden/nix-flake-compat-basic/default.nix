let
  flake = import ./flake-compat.nix { src = ./.; };
in
flake.defaultNix
