#!/usr/bin/env bash

if ! command -v sd >/dev/null 2>&1; then
    echo "You need to install sd (crates.io/crates/sd) to run this setup script!"
    echo "Try running: cargo install sd"
    exit 1
fi

echo "Let's replace some template values with their real values!"
echo "Please note, this template assumes the name of the repo is the same as the name of the crate"
echo "Also, once this runs, make sure to check whether you want Cargo.lock ignored in .gitignore"

REPLACE_IN=(Cargo.toml Cargo.lock README.md src/lib.rs src/main.rs)

read -rp "Enter the crate name: " CRATE_NAME
sd "crate-name" "$CRATE_NAME" "${REPLACE_IN[@]}"

read -rp "Enter the GitHub username this repo belongs to: " USERNAME
sd "username" "$USERNAME" "${REPLACE_IN[@]}"

read -rp "Enter your full name: " FULL_NAME
sd "full-name" "$FULL_NAME" "${REPLACE_IN[@]}"

read -rp "Enter your email address: " EMAIL
sd "email" "$EMAIL" "${REPLACE_IN[@]}"

echo "Removing link chocks..."
sd "fake-link" "" README.md

echo "Self destructing..."
rm setup.sh

read -rp "Commit changes? (y/n): " COMMIT

if [[ $COMMIT == "y" ]]; then
    echo "Committing changes..."
    git add "${REPLACE_IN[@]}" setup.sh
    git commit -m "Set up repository from template"
else
    echo "Not committing changes"
fi
