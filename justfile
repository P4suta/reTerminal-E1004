default:
    @just --list

check:
    cargo run --locked -p xtask -- check

ci profile="host":
    cargo run --locked -p xtask -- ci {{ profile }}

sim *args:
    cargo run --locked -p xtask -- sim {{ args }}

web:
    cargo run --locked -p xtask -- web

web-build:
    cargo run --locked -p xtask -- web-build

firmware features="all":
    cargo run --locked -p xtask -- firmware {{ features }}

firmware-clippy features="all":
    cargo run --locked -p xtask -- firmware-clippy {{ features }}

flash port features="offline":
    cargo run --locked -p xtask -- flash {{ port }} {{ features }}

monitor port:
    cargo run --locked -p xtask -- monitor {{ port }}

proofs:
    cargo run --locked -p xtask -- proofs

mutations:
    cargo run --locked -p xtask -- mutations

fuzz:
    cargo run --locked -p xtask -- fuzz

miri:
    cargo run --locked -p xtask -- miri

docs:
    cargo run --locked -p xtask -- docs

dependencies:
    cargo run --locked -p xtask -- dependencies

browser:
    cargo run --locked -p xtask --features browser -- browser

assurance:
    cargo run --locked -p xtask -- assurance

check-assurance:
    cargo run --locked -p xtask -- check-assurance

verification-image:
    cargo run --locked -p xtask -- verification-image
