readwrite-comp
==============

[![Build Status](https://travis-ci.org/cheme/readwrite-comp.svg?branch=master)](https://travis-ci.org/cheme/readwrite-comp)

Compose read and write stream processing over standard Read and Write.

Extension for ::std::io::Read and ::std::io::Write to stack and chain writers readers.

The library helps composing action on bytes buffer when constrained by a Write interface (for instance cyphering bytes over a tcp writer).

Stacking writers or readers does not require additional buffer.

Support for stacking indefinite number of Write or Read of same type.


Build
-----

Use [cargo](http://crates.io) tool to build and test.

Status
------

Used in [mydht](https://github.com/cheme/mydht).

Documentation
-------------

[API](http://cheme.github.io/readwrite-comp/).
