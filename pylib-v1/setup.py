import re
from setuptools import setup, find_packages


with open('ailets/_version.py', 'r') as f:
    version = re.search(r'__version__\s*=\s*[\'"]([^\'"]*)[\'"]', f.read()).group(1)

setup(
    name="ailets",
    version=version,
    packages=find_packages(),
    install_requires=[
        'aiohttp',
    ],
    author="Oleg Parashchenko",
    author_email="olpa@uucode.com",
    description="Building blocks for realtime AI apps",
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    url="https://github.com/olpa/ailets",
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
    ],
    python_requires=">=3.6",
) 
