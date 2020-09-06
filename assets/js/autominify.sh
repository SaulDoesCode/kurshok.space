#!/usr/bin/bash


# for this to work you need to install "entr" which watches files and runs commands upon changes
ls *.js | entr -c ./minify-all.py