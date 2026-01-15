# pskit-core

This is the core library of the pskit project, designed for processing protein structures. It provides functionalities for annotating, mapping, and splitting protein data.

## Modules

### Annotate
The `annotate` module includes functions and structures for handling protein annotations. It allows users to add and retrieve annotations related to protein structures.

### Map
The `map` module provides functionalities for mapping protein structures. This includes algorithms for visualizing and analyzing the spatial arrangement of atoms within proteins.

### Split
The `split` module offers tools for splitting protein structures into smaller components. This can be useful for analyzing specific regions of interest within a protein.

## Benchmarks
The library includes benchmark tests located in the `benches` directory, specifically for evaluating the performance of neighbor-finding algorithms.

## Testing
Unit tests for the core functionalities are located in the `tests` directory. These tests ensure the reliability and correctness of the library's features.

## Usage
To use this library, include it as a dependency in your `Cargo.toml` file:

```toml
[dependencies]
pskit-core = { path = "../pskit-core" }
```

## License
This project is licensed under the MIT License. See the LICENSE file for more details.