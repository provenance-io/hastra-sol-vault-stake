#!/bin/bash

# Build script for Sol Vault Mint and Stake Docker image

set -e

echo "Building Sol Vault Mint and Stake Docker image..."

# Build the Docker image
docker build -f docker/Dockerfile -t hastra-sol-vault-stake.

echo "Build completed successfully!"
