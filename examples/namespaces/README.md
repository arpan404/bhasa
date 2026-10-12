# Module aliases and privacy

From the repository root:

```sh
./target/release/bhasa run examples/namespaces/src/main.bh
```

The program imports `math.bh` as `math`, calls a public factory, and uses the
result's public `read` method. Its record fields and helper are private to
math.bh. Output is `42`. The Bhasa frontend accepts the same package through
`--frontend bhasa` in a hybrid compiler build.
