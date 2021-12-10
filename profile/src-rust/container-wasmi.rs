//
// Copyright 2021 The Project Oak Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
use std::{env, fs, io::prelude::*, thread, time::Duration};
use wasmi::{ImportsBuilder, Module, ModuleInstance, ModuleRef, NopExternals, RuntimeValue};

const INSTANCE_LIMIT: usize = 100;
const START_DELAY_SECS: u64 = 2;
const LOOP_DELAY_SECS: u64 = 1;

fn main() {
    let module_name = env::args().nth(1).expect("missing module name arg");
    println!("container-wasmi {}", module_name);

    let module = {
        let mut bytes = Vec::new();
        fs::File::open(module_name).unwrap().read_to_end(&mut bytes).unwrap();
        Module::from_buffer(&bytes).unwrap()
    };
    let imports = ImportsBuilder::default();

    let mut instance: Vec<ModuleRef> = Vec::with_capacity(INSTANCE_LIMIT);
    let mut counter = vec![0; INSTANCE_LIMIT];
    thread::sleep(Duration::from_secs(START_DELAY_SECS));

    let mut wi = 0;
    loop {
        if wi < INSTANCE_LIMIT {
            instance.push(ModuleInstance::new(&module, &imports).unwrap().assert_no_start());
            wi += 1;
        }
        for i in 0..wi {
            let res = instance[i].invoke_export("tick", &[], &mut NopExternals).unwrap().unwrap();
            if let RuntimeValue::I32(val) = res {
                counter[i] += 1;
                assert_eq!(val, counter[i]);
            }
        }
        thread::sleep(Duration::from_secs(LOOP_DELAY_SECS));
    }
}
