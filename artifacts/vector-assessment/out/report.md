# Vector asset measurements

- SVG assets found: 7
- Total SVG bytes: 92755
- Total parse time (all assets, cold): 2699 us
- Total fast-raster time (all assets, 3 scales): 11824 us
- Missing file behavior: io_error: No such file or directory (os error 2)
- Corrupt file behavior: parse_error: SVG data parsing failed cause unexpected end of stream

| asset | bytes | parse (us) | fast 1x/2x/3x (us) | high 2x/3x (us) | edge MAE bitmap@2x | edge MAE bitmap@3x | edge MAE fast@2x |
|---|---|---|---|---|---|---|---|
| branding/bevy_bird_dark.svg | 11398 | 871 | 136/243/429 | 4621/10544 | 37.4 | 45.8 | 4.7 |
| branding/bevy_bird_simpleicons.svg | 2185 | 64 | 118/39/45 | 108/187 | 0.0 | 0.0 | 0.0 |
| branding/bevy_logo_dark.svg | 17310 | 249 | 195/332/501 | 8115/20194 | 44.0 | 50.7 | 5.8 |
| branding/bevy_logo_light.svg | 18800 | 412 | 656/1153/1570 | 10054/19756 | 37.7 | 46.9 | 5.6 |
| branding/bevy_logo_light_dark_and_dimmed.svg | 20039 | 523 | 723/1373/2746 | 11114/22451 | 43.8 | 54.4 | 5.8 |
| branding/bevy_solari.svg | 11544 | 372 | 206/280/407 | 6150/12658 | 20.5 | 24.8 | 2.7 |
| branding/icon.svg | 11479 | 208 | 133/219/320 | 4123/8848 | 26.5 | 32.3 | 3.7 |
