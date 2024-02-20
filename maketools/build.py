#!/bin/python3
#
# Copyright 2023 shadow3aaa@gitbub.com
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
#  You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
import os
import shutil
from pathlib import Path
from maketools.toolchains import Buildtools
from maketools.misc import eprint

build_help_text = """\
python3 ./make.py build:
    --help:
        print this help
    --clean:
        clean up
    --release:
        release build
    --debug:
        debug build
    --nightly:
        Introducing more optimizations using rust nightly
    --verbose:
        print details of build\
"""
CFLAGS = (
    "-Ofast -flto -fmerge-all-constants -fno-exceptions -fomit-frame-pointer -fshort-enums \
-Wl,-O3,--lto-O3,--gc-sections,--as-needed,--icf=all,-z,norelro,--pack-dyn-relocs=android+relr \
-std=c++2b -Wall -lc++"
)


def __parse_args(args):
    release = False
    debug = False
    build = False
    verbose = False
    clean = False
    nightly = False

    for arg in args:
        match arg:
            case "--release" | "-r":
                release = True
                build = True
            case "--debug" | "-d":
                debug = True
                build = True
            case "--clean":
                clean = True
            case "--nightly":
                nightly = True
            case "--verbose" | "verbose" | "-v":
                verbose = True
            case "-h" | "--help":
                print(build_help_text)
            case _:
                raise Exception("Illegal build parameter: {}".format(arg))

    if not build and not clean:
        raise Exception(
            "Missing necessary build task argument(--release / --debug / --clean)"
        )
    elif (release and debug) or (build and clean):
        raise Exception("Conflicting build arguments")

    return (clean, release, nightly, verbose)


def __clean():
    try:
        shutil.rmtree("output")
    except Exception:
        pass

    os.system("cargo clean")
    os.chdir("zygisk")

    try:
        shutil.rmtree("output")
    except Exception:
        pass

    os.chdir("rust")
    os.system("cargo clean")


def __build_zygisk(tools: Buildtools, release: bool, verbose: bool, nightly: bool):
    root = Path.cwd()
    zygisk_root = root.joinpath("zygisk")
    os.chdir(zygisk_root)

    try:
        Path("output").mkdir()
    except Exception:
        pass

    os.chdir("rust")

    cargo = tools.cargo().arg("build --target aarch64-linux-android")
    if release:
        cargo = cargo.arg("--release")
    if verbose:
        cargo = cargo.arg("--verbose")
    if nightly:
        cargo = cargo.extra_arg("-Z build-std")

    cargo.build()
    os.chdir(zygisk_root)

    source = Path("rust").joinpath("target").joinpath("aarch64-linux-android")

    if release:
        source = source.joinpath("release")
    else:
        source = source.joinpath("debug")

    source = source.joinpath("librust.a")
    destination = Path("output").joinpath("librust.a")
    shutil.copy2(source, destination)

    output = Path("output").joinpath("arm64-v8a.so")

    (
        tools.cpp()
        .arg("--shared {}".format(Path("src").joinpath("*.cpp")))
        .arg("-I {}".format(Path("rust").joinpath("include")))
        .arg("-L output -L {}".format(Path("..").joinpath("prebuilt")))
        .arg("-fPIC -nostdlib++ -Wl,-lrust,-llog,-lbinder_ndk")
        .arg(CFLAGS)
        .arg("-o {}".format(output))
        .build()
    )

    tools.strip(output)
    os.chdir(root)


def task(args):
    try:
        tools = Buildtools()
    except Exception as err:
        eprint(err)
        exit(-1)

    try:
        (clean, release, nightly, verbose) = __parse_args(args)
    except Exception as err:
        eprint(err)
        exit(-1)

    if clean:
        __clean()
        exit()

    try:
        Path("output").mkdir()
    except Exception:
        pass

    if release:
        temp_dir = Path("output").joinpath(".temp").joinpath("release")
    else:
        temp_dir = Path("output").joinpath(".temp").joinpath("debug")

    try:
        shutil.rmtree(temp_dir)
    except Exception:
        pass

    __build_zygisk(tools, release, nightly, verbose)

    cargo = tools.cargo().arg("build --target aarch64-linux-android")

    if release:
        cargo = cargo.arg("--release")
    if verbose:
        cargo = cargo.arg("--verbose")
    if nightly:
        cargo = cargo.arg("-Z build-std ")

    cargo.build()

    shutil.copytree("module", temp_dir)
    zygisk_lib = Path("zygisk").joinpath("output").joinpath("arm64-v8a.so")
    zygisk_module = temp_dir.joinpath("zygisk")
    zygisk_module.mkdir()
    shutil.copy2(zygisk_lib, zygisk_module)

    bin = Path("target").joinpath("aarch64-linux-android")
    if release:
        bin = bin.joinpath("release")
    else:
        bin = bin.joinpath("debug")
    bin = bin.joinpath("fas-rs")

    bin_module = temp_dir.joinpath("fas-rs")
    shutil.copy2(bin, bin_module)
    tools.strip(bin_module)

    if release:
        output = Path("output").joinpath("fas-rs(release)")
    else:
        output = Path("output").joinpath("fas-rs(debug)")
    shutil.make_archive(output, "zip", temp_dir)
