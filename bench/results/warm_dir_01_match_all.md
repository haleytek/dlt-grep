| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg '.' Downloads` | 56.0 ± 1.9 | 53.8 | 62.0 | 1.00 |
| `dlt-convert(seq) \| grep '.'` | 474.5 ± 1.7 | 471.9 | 476.1 | 8.48 ± 0.29 |
| `dlt-convert(par,32) \| grep '.'` | 168.3 ± 4.0 | 164.8 | 178.1 | 3.01 ± 0.12 |
