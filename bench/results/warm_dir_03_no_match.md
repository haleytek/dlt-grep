| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg '__unlikely_sentinel__' Downloads` | 55.1 ± 1.6 | 52.2 | 60.5 | 1.00 |
| `dlt-convert(seq) \| grep '__unlikely_sentinel__'` | 479.8 ± 5.1 | 472.9 | 487.2 | 8.72 ± 0.27 |
| `dlt-convert(par,32) \| grep '__unlikely_sentinel__'` | 167.3 ± 1.5 | 165.1 | 170.3 | 3.04 ± 0.09 |
