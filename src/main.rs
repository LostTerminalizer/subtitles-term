#![feature(iter_intersperse)]

use std::{
    fs::{read_to_string, File},
    io::{self, Write},
    thread,
    time::{Duration, Instant},
};

use similar::DiffTag;
use soloud::{AudioExt, LoadExt, Soloud};

const TRANSCRIPT: &str = "/home/ved-s/lt/transcripts/1.2.words.tsv";
const SCRIPT: &str = "/home/ved-s/lt/scripts/1.2.txt";
const AUDIO: &str = "/home/ved-s/lt/audio/1.2.mp3";

fn main() {
    let str = read_to_string(TRANSCRIPT).unwrap();
    let transcript_words: Vec<_> = str
        .lines()
        .skip(1)
        .map(|line| {
            let mut split = line.splitn(3, '\t');
            let start: u64 = split.next().unwrap().parse().unwrap();
            let end: u64 = split.next().unwrap().parse().unwrap();
            let phrase = split.next().unwrap();

            (start, end, phrase)
        })
        .collect();
    let script_str = read_to_string(SCRIPT).unwrap();
    let script: Vec<_> = split(&script_str);

    // let transcript_words_str: Vec<_> = transcript_words.iter().map(|s| s.2).collect();
    // let diff = similar::TextDiff::from_slices(&transcript_words_str, &script);
    // let diff = diff.ops();

    let diff: Vec<_> = {
        let fixed_transcript_words: Vec<String> = transcript_words
            .iter()
            .map(|w| {
                w.2.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(|c| c.to_lowercase())
                    .collect()
            })
            .collect();
        let fixed_script_words: Vec<String> = script
            .iter()
            .map(|w| {
                w.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(|c| c.to_lowercase())
                    .collect()
            })
            .collect();

        let fixed_transcript_words: Vec<_> =
            fixed_transcript_words.iter().map(|s| s.as_str()).collect();
        let fixed_script_words: Vec<_> = fixed_script_words.iter().map(|s| s.as_str()).collect();

        let diff = similar::TextDiff::from_slices(&fixed_transcript_words, &fixed_script_words);
        diff.ops().to_vec()
    };

    let ms_per_byte_whole = transcript_words
        .iter()
        .map(|t| {
            let dur = t.1 - t.0;
            dur as f32 / t.2.len() as f32
        })
        .sum::<f32>()
        / transcript_words.len() as f32;

    let average_wait = {
        let mut sum = 0u64;
        let mut count = 0usize;

        for (i, t) in transcript_words.iter().enumerate() {
            if i == 0 {
               continue;
            }

            let wait = t.0 - transcript_words[i-1].1;
            if i > 1 {
                let prev_wait = transcript_words[i-1].0- transcript_words[i-2].1;
                if wait > prev_wait * 20 {
                    continue;
                }
            }

            sum += wait;
            count += 1;
        } 

        if count == 0 {
            0
        } else {
            sum / count as u64
        }
    };

    let mut fixed_script: Vec<(u64, u64, &str)> = vec![];
    for op in diff {
        match op.tag() {
            DiffTag::Equal => {
                for i in 0..op.new_range().len() {
                    let old = op.old_range().start + i;
                    let new = op.new_range().start + i;

                    let old = transcript_words[old];
                    let new = (old.0, old.1, script[new]);

                    fixed_script.push(new);
                }
            }
            DiffTag::Delete => {}
            DiffTag::Insert => 'b: {
                let last_time = fixed_script
                    .iter()
                    .filter_map(|s| if s.1 == 0 { None } else { Some(s.1) })
                    .last();

                if let Some(last_time) = last_time {

                    let script = &script[op.new_range()];

                    let mut pos = last_time;
                    for (i, word) in script.iter().enumerate() {
                        if i > 0 {
                            pos += average_wait
                        }
                        let length = (word.len() as f32 * ms_per_byte_whole) as u64;
                        let end = pos + length;
                        fixed_script.push((pos, end, word));
                        pos += length;
                    }
                    break 'b;
                }

                for i in op.new_range() {
                    fixed_script.push((0, 0, script[i]));
                }
            }
            DiffTag::Replace => 'b: {
                // --[##]-[#]---[####] -> -[###]--[#]-[#####]--
                if op.new_range().len() == op.old_range().len() {
                    for i in 0..op.new_range().len() {
                        let old = op.old_range().start + i;
                        let new = op.new_range().start + i;

                        let old = transcript_words[old];
                        let new = (old.0, old.1, script[new]);
                        fixed_script.push(new);
                    }
                    break 'b;
                }
                // --[#]-[###]---[#]-- -> --[#####]---[##]--
                else {
                    let transcript_words = &transcript_words[op.old_range()];
                    let start = transcript_words[0].0;
                    let end = transcript_words[transcript_words.len() - 1].1;
                    let dur = end - start;

                    let script = &script[op.new_range()];

                    let ms_per_byte = transcript_words
                        .iter()
                        .map(|t| {
                            let dur = t.1 - t.0;
                            dur as f32 / t.2.len() as f32
                        })
                        .sum::<f32>()
                        / transcript_words.len() as f32;

                    let lengths: Vec<_> = script
                        .iter()
                        .map(|word| (word.len() as f32 * ms_per_byte) as u64)
                        .collect();
                    let total_length: u64 = lengths.iter().sum();

                    // all words fit
                    if total_length < dur {
                        let pause_count = script.len() - 1;
                        let pause_len = if pause_count == 0 {
                            0
                        } else {
                            (dur - total_length) / pause_count as u64
                        };
                        let mut pos = start;
                        for (i, word) in script.iter().enumerate() {
                            if i > 0 {
                                pos += pause_len
                            }
                            let length = lengths[i];
                            let end = pos + length;
                            fixed_script.push((pos, end, word));
                            pos += length;
                        }

                        break 'b;
                    }
                }
                // --[#]-[###]--[##] -> --[########]----
                let transcript_words = &transcript_words[op.old_range()];
                let start = transcript_words[0].0;
                let end = transcript_words[transcript_words.len() - 1].1;

                let script = &script[op.new_range()];
                let script_start = script[0];
                let script_end = script[script.len() - 1];

                let script_start = substr_pos(script_str.as_str(), script_start).unwrap();
                let script_end =
                    substr_pos(script_str.as_str(), script_end).unwrap() + script_end.len();

                let script_substr = &script_str[script_start..script_end];

                let phrase = (start, end, script_substr);
                fixed_script.push(phrase);
            }
        }
    }

    {
        let mut file = File::create("test/output.txt").unwrap();
        let mut tmp = String::new();
        for (i, p) in fixed_script.iter().enumerate() {
            if i > 0 {
                file.write_all(b"\n").unwrap();
            }
            tmp.clear();
            for c in p.2.chars() {
                if c == '\n' {
                    tmp.push_str("\\n");
                } else {
                    tmp.push(c);
                }
            }
            file.write_fmt(format_args!("{} {} \"{}\"", p.0, p.1, tmp)).unwrap();
        }
    }

    let sl = Soloud::default().unwrap();
    let mut wav = soloud::Wav::default();
    wav.load(AUDIO).unwrap();

    sl.play(&wav);
    while sl.voice_count() == 0 {
        thread::sleep(Duration::from_millis(1));
    }
    let start = Instant::now();
    for phrase in fixed_script {
        print_phrase(start, phrase.0, phrase.1, phrase.2);
    }
    println!();
    
    if sl.voice_count() > 0 {
        println!();
        println!("[system] Waiting for audio");
        while sl.voice_count() > 0 {
            thread::sleep(Duration::from_millis(100));
        }
    }
}

// fn fit_words_with_set_speed<'a>(
//     script: &'a [&'a str],
//     ms_per_byte: f32,
//     dur: u64,
//     start: u64,
//     fixed_script: &'a mut Vec<(u64, u64, &'a str)>,
// ) -> bool {
//     let lengths: Vec<_> = script
//         .iter()
//         .map(|word| (word.len() as f32 * ms_per_byte) as u64)
//         .collect();
//     let total_length: u64 = lengths.iter().sum();

//     // all words fit
//     if total_length < dur {
//         let pause_count = script.len() - 1;
//         let pause_len = if pause_count == 0 {
//             0
//         } else {
//             (dur - total_length) / pause_count as u64
//         };
//         let mut pos = start;
//         for (i, word) in script.iter().enumerate() {
//             if i > 0 {
//                 pos += pause_len
//             }
//             let length = lengths[i];
//             let end = pos + length;
//             fixed_script.push((pos, end, word));
//             pos += length;
//         }

//         return true;
//     }
//     false
// }

fn split(str: &str) -> Vec<&str> {
    let mut vec = vec![];
    let mut pos = 0;

    while pos < str.len() {
        let sub = &str[pos..];
        let space = sub.find(|c: char| c.is_whitespace());

        let space = match space {
            None => {
                vec.push(sub);
                return vec;
            }
            Some(s) => s,
        };

        let span = &sub[..space];
        if !span.is_empty() {
            vec.push(span);
        }

        if sub.as_bytes()[space] == b'\n' {
            vec.push(&sub[space..space + 1]);
        }
        pos += span.len() + 1;
        while !str.is_char_boundary(pos) {
            pos += 1;
        }
    }
    vec
}

fn substr_pos(main: &str, sub: &str) -> Option<usize> {
    let main_addr = main.as_bytes().as_ptr() as usize;
    let sub_addr = sub.as_bytes().as_ptr() as usize;

    if sub_addr < main_addr || sub_addr > main_addr + main.len() {
        None
    } else {
        Some(sub_addr - main_addr)
    }
}

fn print_phrase(start_time: Instant, start_ms: u64, end_ms: u64, text: &str) {
    let phrase_start_time = start_time + Duration::from_millis(start_ms);
    let phrase_end_time = start_time + Duration::from_millis(end_ms);

    let n = Instant::now();
    let wait = phrase_start_time.checked_duration_since(n);
    if let Some(wait) = wait {
        thread::sleep(wait);
    }

    if phrase_end_time < Instant::now() {
        if text.ends_with('\n') {
            print!("{text}");
        } else {
            print!("{text} ");
        }
        io::stdout().flush().unwrap();
    } else {
        for (i, (text_pos, char)) in text.char_indices().enumerate() {
            let remaining = text.len() - i;
            let remaining_time = phrase_end_time.checked_duration_since(Instant::now());
            let remaining_time = match remaining_time {
                None => {
                    print!("{}", &text[text_pos..]);
                    io::stdout().flush().unwrap();
                    break;
                }
                Some(t) => t,
            };

            let wait = remaining_time / remaining as u32;
            thread::sleep(wait);
            print!("{}", char);
            io::stdout().flush().unwrap();
        }
        if !text.ends_with('\n') {
            print!(" ");
        }
        io::stdout().flush().unwrap();
    }
}
