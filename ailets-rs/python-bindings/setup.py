from setuptools import setup

setup(
    name="ailets-rs",
    version="0.1.0",
    packages=["ailets_rs"],
    install_requires=[
        "wasmer>=1.1.0",
        # or "wasmtime>=1.0.0" if you prefer wasmtime
    ],
    package_data={
        "ailets_rs": ["*.wasm"],
    },
) 