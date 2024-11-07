import os
import sys


selfdir = os.path.dirname(__file__)
libdir = os.path.join(selfdir, "..", "pylib-v1")
libdir = os.path.normpath(os.path.abspath(libdir))

sys.path.append(libdir)

