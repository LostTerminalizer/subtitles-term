#![feature(iter_intersperse)]

use std::{
    collections::VecDeque,
    fmt::Write as _,
    fs::read_to_string,
    sync::Arc,
    time::Instant,
};

use crt_term_gl::ScreenInfo;
use glfw::Context;
use glow::HasContext;
use similar::DiffTag;
use soloud::{AudioExt, LoadExt, Soloud};

const TRANSCRIPT: &str = "lt/1.1.words.tsv";
const SCRIPT: &str = "lt/1.1.txt";
const AUDIO: &str = "lt/1.1.mp3";

pub struct Word {
    pub start_ms: u64,
    pub end_ms: u64,
    pub word: String,
}

pub fn match_timestamps_to_script(words: Vec<Word>, script: &str) -> Vec<Word> {
    let script_words: Vec<_> = split(script);

    let transformed_words: Vec<String> = words
        .iter()
        .map(|w| {
            w.word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect()
        })
        .collect();
    let transformed_script_words: Vec<String> = script_words
        .iter()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(|c| c.to_lowercase())
                .collect()
        })
        .collect();

    let transformed_words: Vec<_> = transformed_words.iter().map(|s| s.as_str()).collect();
    let transformed_script_words: Vec<_> = transformed_script_words
        .iter()
        .map(|s| s.as_str())
        .collect();

    let diff = similar::TextDiff::from_slices(&transformed_words, &transformed_script_words);

    let ms_per_byte_whole = words
        .iter()
        .map(|t| {
            let dur = t.end_ms - t.start_ms;
            dur as f32 / t.word.len() as f32
        })
        .sum::<f32>()
        / words.len() as f32;

    let average_wait = {
        let mut sum = 0u64;
        let mut count = 0usize;

        for (i, t) in words.iter().enumerate() {
            if i == 0 {
                continue;
            }

            let wait = t.start_ms - words[i - 1].end_ms;
            if i > 1 {
                let prev_wait = words[i - 1].start_ms - words[i - 2].end_ms;
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

    let mut matched_words: Vec<Word> = vec![];
    for op in diff.ops() {
        match op.tag() {
            DiffTag::Equal => {
                for i in 0..op.new_range().len() {
                    let old = op.old_range().start + i;
                    let new = op.new_range().start + i;

                    let old = &words[old];
                    let new = Word {
                        start_ms: old.start_ms,
                        end_ms: old.end_ms,
                        word: script_words[new].to_owned(),
                    };

                    matched_words.push(new);
                }
            }
            DiffTag::Delete => {}
            DiffTag::Insert => 'b: {
                let last_time = matched_words
                    .iter()
                    .rev()
                    .filter_map(|s| if s.end_ms == 0 { None } else { Some(s.end_ms) })
                    .next();

                if let Some(last_time) = last_time {
                    let script = &script_words[op.new_range()];

                    let mut pos = last_time;
                    for (i, word) in script.iter().enumerate() {
                        if i > 0 {
                            pos += average_wait
                        }
                        let length = (word.len() as f32 * ms_per_byte_whole) as u64;
                        let end = pos + length;
                        matched_words.push(Word {
                            start_ms: pos,
                            end_ms: end,
                            word: (*word).to_owned(),
                        });
                        pos += length;
                    }
                    break 'b;
                }

                for i in op.new_range() {
                    matched_words.push(Word {
                        start_ms: 0,
                        end_ms: 0,
                        word: script_words[i].to_owned(),
                    });
                }
            }
            DiffTag::Replace => 'b: {
                // --[##]-[#]---[####] -> -[###]--[#]-[#####]--
                if op.new_range().len() == op.old_range().len() {
                    for i in 0..op.new_range().len() {
                        let old = op.old_range().start + i;
                        let new = op.new_range().start + i;

                        let old = &words[old];
                        let new = Word {
                            start_ms: old.start_ms,
                            end_ms: old.end_ms,
                            word: script_words[new].to_owned(),
                        };
                        matched_words.push(new);
                    }
                    break 'b;
                }
                // --[#]-[###]---[#]-- -> --[#####]---[##]--
                else {
                    let words = &words[op.old_range()];
                    let start = words[0].start_ms;
                    let end = words[words.len() - 1].end_ms;
                    let dur = end - start;

                    let script = &script_words[op.new_range()];

                    let ms_per_byte = words
                        .iter()
                        .map(|t| {
                            let dur = t.end_ms - t.start_ms;
                            dur as f32 / t.word.len() as f32
                        })
                        .sum::<f32>()
                        / words.len() as f32;

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
                            matched_words.push(Word {
                                start_ms: pos,
                                end_ms: end,
                                word: (*word).to_owned(),
                            });
                            pos += length;
                        }

                        break 'b;
                    }
                }
                // --[#]-[###]--[##] -> --[########]----
                let words = &words[op.old_range()];
                let start = words[0].start_ms;
                let end = words[words.len() - 1].end_ms;

                let script_words = &script_words[op.new_range()];
                let script_start = script_words[0];
                let script_end = script_words[script_words.len() - 1];

                let script_start = substr_pos(script, script_start).unwrap();
                let script_end = substr_pos(script, script_end).unwrap() + script_end.len();

                let script_substr = &script[script_start..script_end];

                matched_words.push(Word {
                    start_ms: start,
                    end_ms: end,
                    word: script_substr.to_owned(),
                });
            }
        }
    }

    matched_words

    // {
    //     let mut file = File::create("test/output.txt").unwrap();
    //     let mut tmp = String::new();
    //     for (i, p) in fixed_script.iter().enumerate() {
    //         if i > 0 {
    //             file.write_all(b"\n").unwrap();
    //         }
    //         tmp.clear();
    //         for c in p.2.chars() {
    //             if c == '\n' {
    //                 tmp.push_str("\\n");
    //             } else {
    //                 tmp.push(c);
    //             }
    //         }
    //         file.write_fmt(format_args!("{} {} \"{}\"", p.0, p.1, tmp)).unwrap();
    //     }
    // }
}

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

fn main() {
    let str = read_to_string(TRANSCRIPT).unwrap();
    let words: Vec<_> = str
        .lines()
        .skip(1)
        .map(|line| {
            let mut split = line.splitn(3, '\t');
            let start: u64 = split.next().unwrap().parse().unwrap();
            let end: u64 = split.next().unwrap().parse().unwrap();
            let phrase = split.next().unwrap();

            Word {
                start_ms: start,
                end_ms: end,
                word: phrase.to_owned(),
            }
        })
        .collect();
    let script = read_to_string(SCRIPT).unwrap();
    let words = match_timestamps_to_script(words, &script);

    let mut glfw = glfw::init::<()>(None).unwrap();
    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));
    let (mut win, events) = glfw
        .create_window(1280, 720, "subtitles term", glfw::WindowMode::Windowed)
        .unwrap();

    let gl =
        Arc::new(unsafe { glow::Context::from_loader_function(|proc| win.get_proc_address(proc)) });

    let draw_size = win.get_framebuffer_size();

    let default_screen_info = ScreenInfo {
        gl_pos: [-1.0, -1.0],
        gl_size: [2.0, 2.0],

        chars_size: [80, 20],
        frame_size: [0; 2],
    };

    let mut crt = crt_term_gl::CRTTerm::new(
        gl.clone(),
        ScreenInfo {
            frame_size: [draw_size.0 as u32, draw_size.1 as u32],
            ..default_screen_info
        },
    );

    unsafe { gl.clear_color(1.0, 1.0, 1.0, 1.0) };
    win.make_current();
    win.set_framebuffer_size_polling(true);

    let mut remaining = VecDeque::from(words);
    let mut current_word = None;
    let mut current_word_char = 0;
    let mut current_word_next_char_ts = 0;
    let mut current_word_ms_per_char = 0;

    let sl = Soloud::default().unwrap();
    let mut wav = soloud::Wav::default();
    wav.load(AUDIO).unwrap();

    sl.play(&wav);

    let mut start_time = Instant::now();
    let mut started = false;
    let mut printing_space = false;

    let offset_ms = 350;

    while !win.should_close() {
        glfw.poll_events();

        for (_, event) in glfw::flush_messages(&events) {
            if let glfw::WindowEvent::FramebufferSize(width, height) = event {
                unsafe { gl.viewport(0, 0, width, height) };
                crt.screen_changed(ScreenInfo {
                    frame_size: [width as u32, height as u32],
                    ..default_screen_info
                });
            }
        }

        unsafe { gl.clear(glow::COLOR_BUFFER_BIT) };

        if sl.voice_count() != 0 {
            if !started {
                start_time = Instant::now();
            }
            started = true;

            if printing_space {
                if crt.cursor[0] > 0 {
                    let _ = crt.write_char(' ');
                }
                printing_space = false;
            } else {
                let current_ts = (Instant::now() - start_time).as_millis() as u64 + offset_ms;

                if current_word.is_none() {
                    current_word = remaining.pop_front();
                    current_word_char = 0;

                    if let Some(word) = &current_word {
                        current_word_next_char_ts = word.start_ms;
                        if word.end_ms >= word.start_ms {
                            current_word_ms_per_char = 0;
                        } else {
                            current_word_ms_per_char =
                                (word.end_ms - word.start_ms) / word.word.len() as u64;
                        }
                    }
                }

                let mut phrase_ended = false;
                if let Some(word) = &current_word {
                    if current_ts >= current_word_next_char_ts {
                        let char = word.word.chars().nth(current_word_char);
                        if let Some(char) = char {
                            current_word_char += 1;
                            current_word_next_char_ts += current_word_ms_per_char;
                            let _ = crt.write_char(char);
                        } else {
                            phrase_ended = true;
                        }
                    }
                }
                if phrase_ended {
                    current_word = None;
                    printing_space = true;
                }
            }
        }

        crt.update();

        win.swap_buffers();
    }
}
