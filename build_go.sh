#!/bin/bash

# Build script for Go version of MOP

set -e

echo "Building MOP (Go version)..."

# Initialize Go module if needed
if [ ! -f "go.mod" ]; then
    echo "Initializing Go module..."
    go mod init mop
fi

# Download dependencies
echo "Downloading dependencies..."
go mod tidy

# Build the application
echo "Building application..."
go build -o mop .

echo "Build complete! Run with: ./mop"
