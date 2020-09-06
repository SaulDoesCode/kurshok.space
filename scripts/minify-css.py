#!/usr/bin/python
import os
import sys

# MAKE SURE that terser is installed globally with yarn (yarn global add terser)
os.getcwd()

if len(sys.argv) > 1 and ".css" in sys.argv[1]:
    print("\ntrying to minify " + sys.argv[1] + " ...")
    os.system("exec ../../scripts/node_modules/.bin/csso " + sys.argv[1] + " -o " + sys.argv[1][:-3] + "min.css")
    
    print("done minifying, did it work? ")
else:
    print("\nno arg, minifying everything...")
    for root, dirs, files in os.walk("./"):    
        for file in files:
            if ".css" in file and ".min.css" not in file:
                print(file)
                os.system("exec ../../scripts/node_modules/.bin/csso " + file + " -o " + file[:-3] + "min.css")

    print("done, all is minified!")