# Node Module Version Checker

A utility to quickly check whether your installed node_modules actually matches your package.json file(s).

Supports hoisted node_modules with workspaces specified using the "workspaces" package.json attribute.

To install, run
```
cargo install --path .
```

To use, simply navigate to the root of the project and run `mvc` or use the `--dir` flag to specify the project root directory. 