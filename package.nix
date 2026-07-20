# nix file to build dandelion and dandelion tests for lauberhorn for AARCH64
{ lib
, pkgs
, crane
, runtimePkg
}:
let
  craneLib = crane.mkLib pkgs;
  src = ./.;
  
  commonArgs = {
    inherit src;
    pname = "dandelion";
    version = "0.0.1";
    strictDeps = true;
    nativeBuildInputs = with pkgs; [ pkg-config ];
    buildInputs = [
      runtimePkg
      pkgs.libtirpc
    ];
    # Cargo configuration
    cargoExtraArgs = "--features mmu --package dandelion_lauberhorn --package machine_interface";
    RUSTFLAGS = "-C link-arg=-L${runtimePkg} -C link-arg=-L${pkgs.libtirpc}/lib -C link-arg=-Wl,-rpath,${runtimePkg}";
    CARGO_PROFILE = "release";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  testBuild = craneLib.cargoBuild (commonArgs // {
    inherit cargoArtifacts;
    pname = "dandelion-lauberhorn-tests";
    
    cargoBuildFlags = "--tests --no-run";

    installPhase = ''
      mkdir -p $out/tests $out/binaries

      # grab all compiled test binaries from the standard target directory 
      find target/aarch64-unknown-linux-gnu/release/deps -maxdepth 1 -type f -executable \
        | while read -r bin; do
            cp "$bin" $out/tests/
          done

      # TODO: COPY SCIRPT THAT SETS UP ENV VARIABLES FOR TESTING AND LATENCY BINARY
      # Copy binaries and other needed assets
      if [ -d "./machine_interface/tests/data" ]; then
        cp -r ./machine_interface/tests/data/. $out/binaries/
      fi
    '';
  });
in
craneLib.buildPackage (commonArgs // {
  inherit cargoArtifacts;
  
  installPhase = ''
    mkdir -p $out/bin
    cp target/aarch64-unknown-linux-gnu/release/dandelion_lauberhorn $out/bin/
    cp target/aarch64-unknown-linux-gnu/release/mmu_worker $out/bin/
  '';
  
  passthru = {
    tests = testBuild;
  };
})