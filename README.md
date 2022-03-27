Fork of [caller_modpath](https://github.com/Shizcow/caller_modpath) 

The original repo is designed for getting the caller module path of a particular function by performing an external compilation of your application, caching the resulting module path of the caller.

This heavy modification is designed to only perform a single external compilation to aggregate **all** module paths tagged with an attribute which uses `[expose_caller_modpath]`.

This was primarily used for collecting HTTP routes in a modular way without the use of explicit dependencies.
