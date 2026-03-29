SHELL := bash
NIX   := nix-shell -p imagemagick ffmpeg python3 --run

SVG   ?= /var/www/solana.solfunmeme.com/retro-sync/scratch/shamash_bach.svg
OUT   ?= /var/www/solana.solfunmeme.com/retro-sync/scratch/codec-test
REF   := $(OUT)/ref.png

.PHONY: all render transcode extract compare clean

all: render transcode extract compare

$(OUT):
	mkdir -p $(OUT)

# 1. Render SVG → reference PNG
render: $(OUT)
	$(NIX) "convert $(SVG) -resize 1024x1024! $(REF)"
	@echo "✅ ref.png"

# 2. Transcode to all formats
transcode: render
	$(NIX) "convert $(REF) $(OUT)/t.jpg"
	$(NIX) "convert $(REF) -quality 50 $(OUT)/t_q50.jpg"
	$(NIX) "convert $(REF) -quality 20 $(OUT)/t_q20.jpg"
	$(NIX) "convert $(REF) $(OUT)/t.webp"
	$(NIX) "convert $(REF) -quality 50 $(OUT)/t_q50.webp"
	$(NIX) "convert $(REF) $(OUT)/t.gif"
	$(NIX) "convert $(REF) -resize 512x512! $(OUT)/t_512.png"
	$(NIX) "convert $(REF) -resize 256x256! $(OUT)/t_256.png"
	$(NIX) "ffmpeg -y -loop 1 -i $(REF) -t 1 -c:v libx264 -pix_fmt yuv420p -crf 23 $(OUT)/t.mp4"
	$(NIX) "ffmpeg -y -loop 1 -i $(REF) -t 1 -c:v libx264 -pix_fmt yuv420p -crf 40 $(OUT)/t_crf40.mp4"
	$(NIX) "ffmpeg -y -loop 1 -i $(REF) -t 1 -c:v libvpx-vp9 -crf 30 -b:v 0 $(OUT)/t.webm"
	@echo "✅ transcoded"

# 3. Extract frames from video
extract: transcode
	$(NIX) "ffmpeg -y -i $(OUT)/t.mp4 -frames:v 1 $(OUT)/v_mp4.png"
	$(NIX) "ffmpeg -y -i $(OUT)/t_crf40.mp4 -frames:v 1 $(OUT)/v_crf40.png"
	$(NIX) "ffmpeg -y -i $(OUT)/t.webm -frames:v 1 $(OUT)/v_webm.png"
	@echo "✅ extracted"

# 4. PSNR comparison
compare: extract
	$(NIX) "python3 scripts/codec-test.py $(REF) $(OUT)"

clean:
	rm -rf $(OUT)
