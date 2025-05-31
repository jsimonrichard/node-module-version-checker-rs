# Node Module Version Checker

A utility to quickly check whether your installed node_modules actually matches your package.json file(s).

Supports hoisted node_modules with workspaces specified using the "workspaces" package.json attribute.

To install, run
```
cargo install --path .
```

General:
```
Usage: mvc [OPTIONS] <COMMAND>

Commands:
  tree  Show dependency tree for a package
  diff  Compare dependencies between two packages
  help  Print this message or the help of the given subcommand(s)

Options:
  -d, --depth <DEPTH>
  -h, --help           Print help
  -V, --version        Print version
```

Tree:
```
Show the dependency tree for a package

Usage: mvc tree [PACKAGES]...

Arguments:
  [PACKAGES]...

Options:
  -h, --help  Print help
```

Diff:
```
Compare dependencies between two packages

Usage: mvc diff <LEFT> <RIGHT>

Arguments:
  <LEFT>
  <RIGHT>

Options:
  -h, --help  Print help
```