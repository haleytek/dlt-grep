| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg 'error' Downloads` | 55.3 ± 1.5 | 52.5 | 58.3 | 1.00 |
| `dlt-convert(seq) \| grep 'error'` | 475.5 ± 2.1 | 473.2 | 478.9 | 8.60 ± 0.23 |
| `dlt-convert(par,32) \| grep 'error'` | 166.5 ± 1.6 | 163.8 | 169.9 | 3.01 ± 0.08 |
