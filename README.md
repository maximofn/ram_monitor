# RAM monitor

🖥️ RAM Monitor for Ubuntu: The Ultimate Real-Time RAM Tracking Tool. Monitor your RAM temperature directly from your Ubuntu menu bar with RAM Monitor. This user-friendly and efficient application is fully integrated with the latest Ubuntu operating system. Get live updates and optimize your development tasks. Download now and take control of your GPU's health today!

![ram monitor](ram_monitor.gif)

## About GPU Monitor
RAM Monitor is an intuitive tool designed for developers and professionals who need to keep an eye on their RAM health in real time. It integrates seamlessly with the Ubuntu menu bar, providing essential information at your fingertips.

## Key Features
 * Real-time Monitoring: View RAM temperature, all updated live.
 * Optimized for Ubuntu: Crafted to integrate flawlessly with the latest Ubuntu OS.

## Installation

### Clone the repository

```bash
git clone https://github.com/maximofn/ram_monitor.git
```

or with `ssh`

```bash
git clone git@github.com:maximofn/ram_monitor.git
```

### Install the dependencies

Make sure that you do not have any `venv` or `conda` environment installed.

```bash
if [ -n "$VIRTUAL_ENV" ]; then
    deactivate
fi
if command -v conda &>/dev/null; then
    conda deactivate
fi
```
Now install the dependencies

```bash
sudo apt install lm-sensors
```

Select YES to all questions

```bash
sudo sensors-detect
```

Install psensor

```bash
sudo apt install psensor
```

Install psutil

```bash
pip install psutil
```

## Execution at start-up

```bash
ram_monitor.sh
```

## Support

Consider giving a **☆ Star** to this repository, if you also want to invite me for a coffee, click on the following button

[![BuyMeACoffee](https://img.shields.io/badge/Buy_Me_A_Coffee-support_my_work-FFDD00?style=for-the-badge&logo=buy-me-a-coffee&logoColor=white&labelColor=101010)](https://www.buymeacoffee.com/maximofn)
