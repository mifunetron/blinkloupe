# API-format test workflows

ComfyUI workflows in API format that `cargo test` uses (`meta::public::*`) to check BlinkLoupe's graph walk, re-roll, hires and
send-to-workflow against node packs that don't need to be installed. Nothing here is loaded by the app itself.

## From other projects

Five workflows come unchanged from open-source projects, under their licences; the full licence texts are in [licenses/](licenses/).

| File | Source | Licence | Covers |
|---|---|---|---|
| `efficiency-sdxl+imagesaver+was_curiousjp-img2tag2img.json` | [curiousjp/img2tag2img](https://github.com/curiousjp/img2tag2img) `workflows/i2t2i.sdxl.raw.json` | MIT, Copyright (c) 2024 curious.jp | Eff. Loader SDXL, KSampler SDXL (Eff.), Pack / Unpack SDXL Tuple, Image Saver, WAS Seed |
| `impact-basicpipe+efficiency_charlweed-gimp-comfyui.json` | [Charlweed/gimp_comfyui](https://github.com/Charlweed/gimp_comfyui) `assets/flux_neg_upscale_sdxl_0.5_workflow_api.json` | MIT, Copyright (c) 2024 Charles Hymes | ToBasicPipe, FromBasicPipe_v2, ImpactKSamplerAdvancedBasicPipe |
| `vhs-videocombine+core-saveanimatedwebp_fofr-cog-comfyui-mochi.json` | [fofr/cog-comfyui](https://github.com/fofr/cog-comfyui) `examples/api_workflows/mochi_api.json` | MIT, Copyright (c) 2024 fofr | VHS_VideoCombine, SaveAnimatedWEBP |
| `vhs-loadvideo+videocombine_fofr-cog-comfyui-flashvsr.json` | [fofr/cog-comfyui](https://github.com/fofr/cog-comfyui) `examples/api_workflows/FlashVSR_api.json` | MIT, Copyright (c) 2024 fofr | VHS_LoadVideo |
| `core-savevideo-dynamiccombo_eliovp-paiton-wan22.json` | [Eliovp-BV/paiton-vllm-plugin](https://github.com/Eliovp-BV/paiton-vllm-plugin) `models/Wan2.2/comfyui/workflows/base.api.json` | Apache-2.0 | SaveVideo (dotted format.codec keys) |

## Written for BlinkLoupe

Seven workflows (`*_own.json`) were written for these tests. They use the same node packs' inputs, with their own models,
prompts, seeds and sizes; the model and LoRA names are made up. They're covered by this repository's licence.

| File | Covers |
|---|---|
| `efficiency-adv+lorastacker+was-imagesave_own.json` | Efficient Loader + LoRA Stacker (simple, 5 slots) -> KSampler Adv. (Efficient) + HighRes-Fix Script; WAS Text Concatenate of ttN text; WAS Image Save |
| `efficiency-ksampler+instantid+ipadapter_own.json` | KSampler (Efficient) through ApplyInstantID and IPAdapterApplyFaceID; one Load Image |
| `imagesaver+impact-wildcard+rgthree-seed_own.json` | ImpactWildcardEncode x2, easy ckptNames / int / showAnything, Seed (rgthree), Image Saver with its Sampler / Scheduler Selectors, ImpactSwitch |
| `imagesaver-simple+rgthree-powerlora+subgraph_own.json` | Image Saver Simple + Metadata, an empty Power Lora Loader, subgraph node ids ("40:6"), an empty prompt, ConditioningZeroOut |
| `rgthree-powerlora-lora1+addtextprefix_own.json` | Power Lora Loader lora_1, AddTextPrefix + String Literal; no Load Image |
| `comfyroll-lorastack+searge-sdxl_own.json` | CR LoRA Stack (a slot Off) -> CR Apply LoRA Stack; SeargeSDXLSampler base + refiner img2img |
| `core-savevideo-oldformat+samplercustom_own.json` | SamplerCustomAdvanced + BasicGuider, LoraLoaderModelOnly then an empty Power Lora Loader, CreateVideo, SaveVideo (flat format / codec) |
