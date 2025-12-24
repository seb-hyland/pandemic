#!/bin/bash

set -eu pipefail

trunk build --release
rm -rf docs/
mv dist/ docs/
