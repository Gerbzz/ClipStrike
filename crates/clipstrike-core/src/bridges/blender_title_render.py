import bpy
import sys
import argparse
import os

def render_title(text, output_dir):
    # This is a generic script to be used with a .blend template
    # It assumes there is a text object named 'TitleText'
    
    if 'TitleText' in bpy.data.objects:
        bpy.data.objects['TitleText'].data.body = text
    
    # Set output path
    bpy.context.scene.render.filepath = os.path.join(output_dir, "frame_")
    bpy.context.scene.render.image_settings.file_format = 'PNG'
    
    # Render animation
    bpy.ops.render.render(animation=True)

if __name__ == "__main__":
    # Get arguments after '--'
    try:
        idx = sys.argv.index("--")
        args = sys.argv[idx+1:]
    except ValueError:
        args = []

    parser = argparse.ArgumentParser()
    parser.add_argument("--text", default="ClipStrike 3D")
    parser.add_argument("--output-dir", required=True)
    
    parsed_args = parser.parse_known_args(args)[0]
    
    render_title(parsed_args.text, parsed_args.output_dir)
