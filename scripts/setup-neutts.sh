#!/usr/bin/env bash
# Convenience wrapper — delegates to download-models.sh neutts
exec "$(dirname "$0")/download-models.sh" neutts
