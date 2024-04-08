#!/usr/bin/env python3
import signal
import gi
gi.require_version('AppIndicator3', '0.1')
from gi.repository import AppIndicator3, GLib
from gi.repository import Gtk as gtk
import os
import webbrowser
import psutil
import matplotlib.pyplot as plt
from PIL import Image
import time
import re

APPINDICATOR_ID = 'GPU_monitor'

image_to_show = None
old_image_to_show = None

def main():
    path = os.path.dirname(os.path.realpath(__file__))
    icon_path = os.path.abspath(f"{path}/ram.png")
    RAM_indicator = AppIndicator3.Indicator.new(APPINDICATOR_ID, icon_path, AppIndicator3.IndicatorCategory.SYSTEM_SERVICES)
    RAM_indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
    RAM_indicator.set_menu(build_menu())

    # Get disk info
    GLib.timeout_add_seconds(1, update_ram_info, RAM_indicator)

    GLib.MainLoop().run()

def open_repo_link(_):
    webbrowser.open('https://github.com/maximofn/ram_monitor')

def buy_me_a_coffe(_):
    webbrowser.open('https://www.buymeacoffee.com/maximofn')

def build_menu():
    menu = gtk.Menu()

    memory_info = get_ram_info()

    memory_free = gtk.MenuItem(label=f"Free: {memory_info['free']:.2f} GB")
    menu.append(memory_free)

    memory_used = gtk.MenuItem(label=f"Used: {memory_info['used']:.2f} GB")
    menu.append(memory_used)

    memory_total = gtk.MenuItem(label=f"Total: {memory_info['total']:.2f} GB")
    menu.append(memory_total)

    horizontal_separator1 = gtk.SeparatorMenuItem()
    menu.append(horizontal_separator1)

    item_repo = gtk.MenuItem(label='Repository')
    item_repo.connect('activate', open_repo_link)
    menu.append(item_repo)

    item_buy_me_a_coffe = gtk.MenuItem(label='Buy me a coffe')
    item_buy_me_a_coffe.connect('activate', buy_me_a_coffe)
    menu.append(item_buy_me_a_coffe)

    horizontal_separator2 = gtk.SeparatorMenuItem()
    menu.append(horizontal_separator2)

    item_quit = gtk.MenuItem(label='Quit')
    item_quit.connect('activate', quit)
    menu.append(item_quit)

    menu.show_all()
    return menu

def update_ram_info(indicator):
    global image_to_show
    global old_image_to_show

    get_ram_info()

    # Show pie chart
    path = os.path.dirname(os.path.realpath(__file__))
    icon_path = os.path.abspath(f"{path}/{image_to_show}")
    indicator.set_icon_full(icon_path, "RAM Usage")
    
    old_image_to_show = image_to_show

    return True

def get_ram_info():
    global image_to_show
    global old_image_to_show

    vm = psutil.virtual_memory()
    total_gb = vm.total / 1024**3
    free_gb = vm.available / 1024**3
    used_gb = vm.used / 1024**3

    blue_color = '#66b3ff'
    red_color = '#ff6666'
    green_color = '#99ff99'
    orange_color = '#ffcc99'
    yellow_color = '#ffdb4d'
    percentage_warning1 = 70
    percentage_warning2 = 80
    percentage_caution = 90

    # Create pie chart
    labels = 'Used', 'Free'
    sizes = [used_gb / total_gb * 100, free_gb / total_gb * 100]  # Percentage of used and free memory
    percentage_of_use = sizes[0]

    if percentage_of_use < percentage_warning1:
        used_color = green_color
    elif percentage_of_use >= percentage_warning1 and percentage_of_use < percentage_warning2:
        used_color = yellow_color
    elif percentage_of_use >= percentage_warning2 and percentage_of_use < percentage_caution:
        used_color = orange_color
    else:
        used_color = red_color
    total_color = blue_color
    colors = [used_color, total_color]
    explode = (0.1, 0)  # Explode used memory

    fig, ax = plt.subplots()
    ax.pie(sizes, explode=explode, labels=labels, colors=colors, autopct='%1.1f%%',
           startangle=90, pctdistance=0.85, counterclock=False, wedgeprops=dict(width=0.3, edgecolor='w'))

    # Dibuja un círculo en el centro para convertir el gráfico de PIE en un gráfico de DONA con centro transparente
    centre_circle = plt.Circle((0,0),0.70,fc='none', edgecolor='none')
    fig = plt.gcf()
    fig.gca().add_artist(centre_circle)

    ax.axis('equal')  # Mantiene la relación de aspecto igual para que se dibuje como un círculo
    plt.tight_layout()

    # Save pie chart
    path = os.path.dirname(os.path.realpath(__file__))
    plt.savefig(f"{path}/ram_chart.png", transparent=True)
    plt.close(fig)

    # Define icon height
    icon_height = 22
    padding = 10

    # Load íconos
    ram_icon = Image.open(f'{path}/ram.png')
    ram_chart = Image.open(f'{path}/ram_chart.png')

    # Resize icons
    ram_icon_relation = ram_icon.width / ram_icon.height
    ram_icon_width = int(icon_height * ram_icon_relation)
    scaled_ram_icon = ram_icon.resize((ram_icon_width, icon_height), Image.LANCZOS)

    # Resize chart
    chart_icon_relation = ram_chart.width / ram_chart.height
    chart_icon_width = int(icon_height * chart_icon_relation)
    scaled_ram_chart = ram_chart.resize((chart_icon_width, icon_height), Image.LANCZOS)

    # New image with the combined icons
    total_width = scaled_ram_icon.width + scaled_ram_chart.width
    combined_image = Image.new('RGBA', (total_width, icon_height+padding), (0, 0, 0, 0))  # Fondo transparente

    # Combine icons
    combined_image.paste(scaled_ram_icon, (0, int(padding/2)))
    combined_image.paste(scaled_ram_chart, (scaled_ram_icon.width, int(padding/2)), scaled_ram_chart)

    # Save combined image
    timestamp = int(time.time())
    image_to_show = f'ram_info_{timestamp}.png'
    combined_image.save(f'{path}/{image_to_show}')

    # Remove old image
    if os.path.exists(f'{path}/{old_image_to_show}'):
        os.remove(f'{path}/{old_image_to_show}')

    return {"total": total_gb, "free": free_gb, "used": used_gb}

if __name__ == "__main__":
    path = os.path.dirname(os.path.realpath(__file__))

    if os.path.exists(f'{path}/ram_info.png'):
        os.remove(f'{path}/ram_info.png')
    
    # Remove all ram_info_*.png files
    for file in os.listdir(path):
        if re.search(r'ram_info_\d+.png', file):
            os.remove(f'{path}/{file}')

    signal.signal(signal.SIGINT, signal.SIG_DFL) # Allow the program to be terminated with Ctrl+C
    main()
