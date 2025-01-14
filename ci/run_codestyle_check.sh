#!/bin/bash

cargo fmt --check \
  -p runtime_config \
  -p system_contracts \
  -p primitives \
  -p xscore
