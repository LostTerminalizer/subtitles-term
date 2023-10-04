echo "Input name: $1"
echo "Input duration: $(ffprobe -i /var/subtitles/$1 -show_entries format=duration -v quiet -sexagesimal -of csv="p=0")"
echo ""
whisper_timestamped /var/subtitles/$1 --model small --output_dir=/var/subtitles/output --language en --output_format tsv --punctuations_with_words False --temperature_increment_on_fallback 0 --model_dir /var/subtitles/models --efficient --recompute_all_timestamps True