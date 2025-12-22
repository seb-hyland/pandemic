#!/bin/bash

set -eu pipefail

trunk build --release
rm -r docs/
mv dist/ docs/
