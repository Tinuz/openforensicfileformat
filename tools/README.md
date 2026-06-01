# Tools

## Purpose
This directory contains tooling assets used locally for development and smoke validation.

## Current Contents
- `ewfexport-docker/`: Docker build context for an EWF tooling image.

## Usage
1. Build the tooling image:

```bash
docker build -t offf/ewf-tools:latest tools/ewfexport-docker
```

2. Use this image for E01 export flows in local smoke tests.

## Guidelines
- Keep tool images small and reproducible.
- Pin versions where possible.
- Document command-line behavior in the relevant subdirectory.
