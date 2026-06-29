| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |
|:---|---:|---:|---:|---:|
| `dg '__unlikely_sentinel__' Downloads` | 319.9 ± 61.2 | 277.4 | 425.0 | 1.65 ± 0.32 |
| `dlt-convert(seq) \| grep '__unlikely_sentinel__'` | 548.3 ± 79.3 | 503.0 | 689.7 | 2.83 ± 0.42 |
| `dlt-convert(par,32) \| grep '__unlikely_sentinel__'` | 193.8 ± 6.2 | 186.6 | 201.8 | 1.00 |
