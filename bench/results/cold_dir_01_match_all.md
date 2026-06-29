| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg '.' Downloads` | 286.5 ± 16.9 | 267.3 | 313.5 | 1.50 ± 0.10 |
| `dlt-convert(seq) \| grep '.'` | 515.4 ± 7.1 | 509.3 | 525.6 | 2.70 ± 0.10 |
| `dlt-convert(par,32) \| grep '.'` | 191.0 ± 6.8 | 183.2 | 201.2 | 1.00 |
