| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg 'error' Downloads` | 307.6 ± 30.4 | 281.5 | 352.8 | 1.63 ± 0.16 |
| `dlt-convert(seq) \| grep 'error'` | 512.4 ± 8.2 | 499.9 | 519.4 | 2.71 ± 0.05 |
| `dlt-convert(par,32) \| grep 'error'` | 188.9 ± 1.8 | 187.2 | 191.2 | 1.00 |
