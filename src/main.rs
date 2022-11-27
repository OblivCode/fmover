#![allow(non_snake_case)]

use std::{collections::{HashMap}, fs::{self, File}, io::{self, Read, BufReader, BufWriter, Write}, path::Path, env};
use sysinfo::{SystemExt};
use walkdir::WalkDir;

const MEGABYTE: u64 = 1048576;
const MIN_MEMORY: u64 =  100 * MEGABYTE as u64; //minimum memory to run


const DEBUG: bool = true;



fn main() {

    //check minimum memory requirement
    if GetFreeMemory() < MIN_MEMORY {
        print!("Not enough memory! {} MBs minimum required!", MIN_MEMORY/1000000);
        return 
    }

    let mut cmd_args: Vec<String> = env::args().collect();
    cmd_args.remove(0);

    

    //calculate how much memory to try leave free
    let mut free_memory_percent: u64 = 10; //percentage of memory to leave
    let mut memory_to_leave: u64 = 7000 * MEGABYTE; //(GetFreeMemory()/100) * FREE_MEMORY_PERCENT;
    let mut file_size_limit = 0;

    


    
    let (mut input_path, mut output_path): (String, String) = ("".to_owned(),"".to_owned());
    let mut big_filenames: Vec<String> = Vec::new(); //files that are over the
    let mut directories: Vec<String> = Vec::new();
    let mut file_entries: HashMap<String, Vec<u8>> = HashMap::new();

    if cmd_args.len() > 0{
        if !cmd_args[0].starts_with("-") { input_path = cmd_args[0].to_string() } 
        if !cmd_args[1].starts_with("-") { output_path = cmd_args[1].to_string() }

        for (index, arg) in cmd_args.iter().enumerate() {
            let next: &String;
            if arg == "-file_limit" {
                next = &cmd_args[index+1];
                file_size_limit = next.parse::<u64>().expect("Could not parse argument (string->u64)");
            }
            else if arg == "-memory_leave" {
                next = &cmd_args[index+1];
                memory_to_leave = next.parse::<u64>().expect("Could not parse argument (string->u64)");
            }
            else if arg == "-memory_leave_percent" {
                next = &cmd_args[index+1];
                let fmp = next.parse::<u64>().expect("Could not parse argument (string->u64)");

                if fmp > 100 { panic!("-memory_leave_percent must be  a percentage! (from 1 to 100)")}

                free_memory_percent = fmp;
            }
        }
    }

   if DEBUG {
    println!("Memory to leave: {}% ({} MBs)", free_memory_percent, &memory_to_leave/MEGABYTE);
    println!("File size limit: {} bytes", &file_size_limit);
   }
    
    if input_path == "" {
        println!("Directory to move files from: ");
        input_path = ReadInput();
    }

    if output_path == "" {
        println!("Directory to move files to: ");
        output_path = ReadInput();
    }
 
    if output_path.ends_with("/") {
        output_path.remove(output_path.len()-1);
    }


    let dir_entries = WalkDir::new(&input_path);
    for entry in dir_entries {
        let path_buffer = entry.expect("Could not get entry.").path().to_owned();
        let path = path_buffer.to_str().expect("Path buffer returned none!").to_owned();
        let relative_path = path.replace(&input_path, "").trim().to_owned();

        if DEBUG {
            println!("Reading {}", relative_path)
        }

        if path_buffer.is_file() {
            let data: Vec<u8>;
            let metadata = path_buffer.metadata().expect(&format!("Could not get file metadata: {}", relative_path));
            let file_size = metadata.len();

            //check if memory limit reached
            if GetFreeMemory() < memory_to_leave + file_size {
                if directories.len() >  0 {
                    if DEBUG {println!("Working directories")}
                    WorkDirectories(&mut directories, &output_path)
                }
                WorkFileEntries(&mut file_entries,&input_path, &output_path);
                file_entries.clear();
            }

            //if file too big to be read at once
            if file_size as i64 > (GetFreeMemory()as i64 - memory_to_leave as i64)  || (file_size_limit != 0 && file_size > file_size_limit) { 
                if DEBUG {
                    let size_gb: f64 = (file_size as f64)/1000000000.0;
                    println!("BIG FILE! {:.1} gigabytes", size_gb);
                    
                }

                big_filenames.push(path);
                continue;
            }
            else { //read file
                if DEBUG {
                    let size_kb: f64 = (file_size as f64)/1000.0;
                    println!("Reading {:.3}  kilobytes", size_kb)
                }
                data = ReadFile(&path);
                file_entries.insert(relative_path, data);
            }
        }
        else {
            directories.push(relative_path);
            if DEBUG {println!("Directory")}
        }
    }

    
    if directories.len() >  0 {
        if DEBUG {println!("Working final directories")}
        WorkDirectories(&mut directories, &output_path)
    }
    

    if DEBUG {println!("Working final files")}
    if (&file_entries).len() > 0 { 
        WorkFileEntries(&mut file_entries,&input_path, &output_path); 
       }

    if (&big_filenames).len() > 0 {
        if DEBUG {println!("Working BIG files", )}

        WorkBigFiles(&mut big_filenames, &memory_to_leave, &input_path, &output_path, &file_size_limit);
    }

    println!("Done!");
}

fn WorkDirectories(directories: &mut Vec<String>, output_path: &str) {
    for dir in directories.iter() {
        if DEBUG {println!("Creating {}", dir);}
        let new_path = format!("{}{}", output_path, dir);
        fs::create_dir_all(&new_path).expect(&format!("Could not create directory path: {}", &new_path));
    }
    directories.clear();
}

fn WorkBigFiles(big_files: &mut Vec<String>, memory_to_leave: &u64, input_path: &str, output_path: &str, file_size_limit: &u64) {
    let (mut relative_path, mut new_path, ): (String, String);
    let(mut buffer, mut chunk_count, mut file_size, mut bytes_read): (Vec<u8>, u64, u64, u64);

    for filename in big_files.iter() {
        let free_space = GetFreeMemory() - memory_to_leave;
        //if no limit then use free space otherwise use limit
        let bytes_to_read_default = if file_size_limit.eq(&0) {free_space} else { *std::cmp::min(file_size_limit, &free_space)};

        relative_path = filename.replace(&input_path, "").trim().to_owned();
        new_path = format!("{}{}", output_path, relative_path);
        file_size = Path::new(filename).metadata().expect(&format!("Could not get file metadata: {}", relative_path)).len();

        if DEBUG {
            println!("{}", relative_path);
            println!("File size: {}", file_size);
            println!("Bytes to read default: {}", bytes_to_read_default);
        }

        chunk_count = (file_size as f64 / bytes_to_read_default as f64).ceil() as u64;
        bytes_read = 0;

        if DEBUG {println!("Split into {} chunks", chunk_count)}

        let old_fs = File::open(filename).expect(&format!("Could not open file: {}", filename));
        let new_fs = File::create(new_path).expect("Could not create new file");

        let mut reader = BufReader::new(old_fs);
        let mut writer = BufWriter::new(new_fs);

        for count in 0..chunk_count {
            let bytes_to_read = if file_size > bytes_read { 
                let bytes_left = file_size - bytes_read;
                std::cmp::min(bytes_to_read_default, bytes_left)
             } else {0};
            //if no more bytes left
            if bytes_to_read == 0 {break}

            buffer = vec![0u8; bytes_to_read as usize];

            if DEBUG { println!("Reading chunk {}: {} bytes", count+1, bytes_to_read); }
           
            //read and write chunk
            reader.read_exact(&mut buffer).expect("Failed to read from stream");
            writer.write(&buffer).expect("Failed to write to stream");

            bytes_read += buffer.len() as u64;
        }
    }
    big_files.clear();
}

fn WorkFileEntries(file_entries: &mut HashMap<String, Vec<u8>>, input_path: &str, output_path: &str) {
    let (mut relative_path, mut new_path): (String, String);

        for (filename, data) in file_entries.iter() {
            relative_path = filename.replace(&input_path, "").trim().to_owned();
            new_path = format!("{}{}", output_path, relative_path);

            if DEBUG {println!("Writing {}", relative_path)}

            fs::write(&new_path, &data).expect("Could not write to new file");
        }
        file_entries.clear();
}
fn ReadInput() -> String {
    let mut buffer = String::new();

    io::stdin().read_line(&mut buffer)
    .expect("Could not read from stdin.");

    return buffer.trim().to_string();
}
fn ReadFile(filename: &str) -> Vec<u8> {
    fs::read(filename)
    .expect(&format!("Could not read file: {}", filename))
}

fn GetFreeMemory() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_all();
    return system.free_memory();
}


