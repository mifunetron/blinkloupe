"""Synthetic frames for trying Loupe without touching real output:
  testdata/watch/01_comfy_pears.png   flat colour + a working ComfyUI graph (Muse chain, saves to output/tests/loupe-selftest)
  testdata/watch/02_a1111.png         gradient + an A1111 parameters block
  testdata/watch/03_black.png         a black frame (the BLANK alarm)
  testdata/watch/sub/04_photo.jpg     a JPEG in a subfolder, no metadata
Usage: python make_testdata.py [--graph-from <png with a ComfyUI prompt chunk>]   (only the graph's node layout is reused)
       python make_testdata.py --add <name>   writes one more frame (to watch LIVE pick it up)"""
import json, os, sys
from PIL import Image, ImageDraw, PngImagePlugin

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "testdata", "watch")

PEARS = "a still life of three green pears on a scrubbed wooden table, soft window light from the left, muted background, oil painting"


def comfy_graph():
    return {
        "1": {"class_type": "UnetLoaderGGUFDynamicVRAM", "inputs": {"unet_name": "museByStableYogi_v20TurboINT8.gguf"}},
        "2": {"class_type": "LoraLoaderModelOnly", "inputs": {"model": ["1", 0], "lora_name": "krea2filterbypass3.safetensors", "strength_model": 1.0}},
        "5": {"class_type": "CLIPLoader", "inputs": {"clip_name": "qwen3vl_4b_fp8_scaled.safetensors", "type": "krea2", "device": "default"}},
        "6": {"class_type": "VAELoader", "inputs": {"vae_name": "qwen_image_vae.safetensors"}},
        "7": {"class_type": "CLIPTextEncode", "inputs": {"text": PEARS, "clip": ["5", 0]}},
        "8": {"class_type": "ConditioningZeroOut", "inputs": {"conditioning": ["7", 0]}},
        "9": {"class_type": "EmptyLatentImage", "inputs": {"width": 768, "height": 768, "batch_size": 1}},
        "10": {"class_type": "KSampler", "inputs": {"model": ["2", 0], "seed": 4242, "steps": 8, "cfg": 1.0, "sampler_name": "euler",
                                                     "scheduler": "simple", "positive": ["7", 0], "negative": ["8", 0],
                                                     "latent_image": ["9", 0], "denoise": 1.0}},
        "11": {"class_type": "VAEDecode", "inputs": {"samples": ["10", 0], "vae": ["6", 0]}},
        "12": {"class_type": "SaveImage", "inputs": {"images": ["11", 0], "filename_prefix": "tests/loupe-selftest/pears"}},
    }


def save_png(name, im, text):
    info = PngImagePlugin.PngInfo()
    for k, v in text.items():
        info.add_text(k, v)
    p = os.path.join(OUT, name)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    im.save(p, pnginfo=info)
    print("wrote", os.path.normpath(p))


def flat(w, h, rgb, label):
    im = Image.new("RGB", (w, h), rgb)
    d = ImageDraw.Draw(im)
    for i in range(0, w, 64):
        d.line((i, 0, i, h), fill=tuple(min(255, c + 18) for c in rgb), width=2)
    d.text((40, 40), label, fill=(255, 255, 255))
    return im


def main():
    if "--add" in sys.argv:
        name = sys.argv[sys.argv.index("--add") + 1]
        save_png(name, flat(900, 1200, (90, 60, 120), "LIVE " + name), {"parameters": "a live test frame\nSteps: 4, Sampler: Euler, CFG scale: 1, Seed: 7, Size: 900x1200"})
        return
    save_png("01_comfy_pears.png", flat(768, 768, (60, 96, 70), "ComfyUI graph"), {"prompt": json.dumps(comfy_graph())})
    grad = Image.linear_gradient("L").resize((832, 1216)).convert("RGB")
    save_png("02_a1111.png", grad, {"parameters": "a castle <lora:mossy:0.7> at night, fog\nNegative prompt: lowres\n"
                                                    "Steps: 20, Sampler: DPM++ 2M, Schedule type: Karras, CFG scale: 5, Seed: 42, Size: 832x1216, Model: juggernautXL"})
    save_png("03_black.png", Image.new("RGB", (640, 640), (2, 2, 3)), {})
    os.makedirs(os.path.join(OUT, "sub"), exist_ok=True)
    flat(1200, 800, (150, 110, 60), "JPEG in a subfolder").save(os.path.join(OUT, "sub", "04_photo.jpg"), quality=90)
    print("wrote", os.path.normpath(os.path.join(OUT, "sub", "04_photo.jpg")))


if __name__ == "__main__":
    main()
