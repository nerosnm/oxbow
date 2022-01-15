{ lib
, stdenv
, wip ? false
, zola
, ...
}:

stdenv.mkDerivation rec {
  pname = "oxbow-cacti-dev";
  version = "0.1.0";

  src = ./.;

  buildInputs = [ zola ];

  buildPhase = ''
    zola build
  '';

  installPhase = ''
    cp -r public $out
  '';
}
