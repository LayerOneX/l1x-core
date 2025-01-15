use std::collections::HashMap;
use std::path::PathBuf;

use cranelift_wasm::wasmparser::{
    FuncValidator, FunctionBody, Operator, ValType, ValidatorResources,
};
use cranelift_wasm::{
    self, DataIndex, ElemIndex, FuncIndex, Global, GlobalIndex, GlobalInit, Memory, MemoryIndex,
    ModuleEnvironment, Table, TableIndex, TypeIndex, WasmError, WasmFuncType, WasmResult, WasmType,
};
use inkwell::attributes::{Attribute, AttributeLoc};
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::memory_buffer::MemoryBuffer;
use inkwell::module::{Linkage, Module};
use inkwell::types::{ArrayType, BasicMetadataTypeEnum, BasicType, IntType};
use inkwell::values::{BasicMetadataValueEnum, FunctionValue, GlobalValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};

use log::debug;

const LOOP_START_NAME: &str = "Loop";
const LOOP_END_NAME: &str = "LoopEnd";
const LOOP_PARAM_VAR_NAME: &str = "loop_par";
const LOOP_RESULT_VAR_NAME: &str = "loop_res";
const IF_THEN_NAME: &str = "Then";
const IF_ELSE_NAME: &str = "Else";
const IF_END_NAME: &str = "IfEnd";
const IF_RESULT_VAR_NAME: &str = "if_res";
const BLOCK_END_NAME: &str = "BlockEnd";
const BLOCK_RESULT_VAR_NAME: &str = "block_res";
const FUNCTION_ENTRY_NAME: &str = "Entry";
const FUNCTION_END_NAME: &str = "End";
const FUNCTION_RESULT_VAR_NAME: &str = "fn_res";
const FUNCTION_PARAM_NAME: &str = "p";
const FUNCTION_LOCAL_VAR_NAME: &str = "l";
const FUNCTION_TMP_VAR_NAME: &str = "t";
const INDIRECT_CALL_CASE_NAME: &str = "CallIdr";
const INDIRECT_CALL_ELSE_NAME: &str = "CallIdrElse";
const INDIRECT_CALL_END_NAME: &str = "CallIdrEnd";
const INDIRECT_CALL_RESULT_VAR_NAME: &str = "cidr_res";
const GLOBAL_VAR_NAME: &str = "GVar";
const TABLE_VAR_NAME: &str = "GTab";
const FN_PARAM_VAR_NAME: &str = "GFnPar";

const MEMORY_VAR_NAME: &str = "GMem";
const INIT_MEMORY_VAR_NAME: &str = "GInitMem";
const MEMORY_SECTION_NAME: &str = "_memory";
const INIT_MEMORY_SECTION_NAME: &str = "_init_memory";

struct LocalVarBuilder<'a> {
    basic_block: BasicBlock<'a>,
    ty: IntType<'a>,
}

impl<'a> LocalVarBuilder<'a> {
    fn new(basic_block: BasicBlock<'a>, ty: IntType<'a>) -> Self {
        Self { basic_block, ty }
    }

    fn get(&self, llvm_builder: &'a Builder, name: &str) -> PointerValue<'a> {
        let old_bb = llvm_builder.get_insert_block().unwrap();
        if let Some(instruction) = self.basic_block.get_first_instruction().as_ref() {
            llvm_builder.position_before(instruction);
        } else {
            llvm_builder.position_at_end(self.basic_block);
        }
        let ptr = llvm_builder.build_alloca(self.ty, name);
        llvm_builder.position_at_end(old_bb);
        ptr
    }
}

#[derive(Debug)]
enum SyscallMathOp {
    F64ConvertI64U = 1,
    F64Add,
    F64Sub,
    F64Mul,
    F64Div,
    F64Eq,
    F64Ne,
    F64Neg,
    F64Gt,
    F64Ge,
    F64Le,
    F64Lt,
    F32Add,
    F32Sub,
    F32Mul,
    F32Div,
    F32Eq,
    F32Ne,
    F32Neg,
    F32Gt,
    F32Ge,
    F32Le,
    F32Lt,
    I64DivS,
    I32DivS,
    I64RemS,
    I32RemS,
    I64Clz,
    I32Clz,
    I64Ctz,
    I32Ctz,
    F64ConvertI32S,
    F64ConvertI32U,
    F64Abs,
    F32Abs,
}

impl Into<u64> for SyscallMathOp {
    fn into(self) -> u64 {
        self as u64
    }
}

impl From<&Operator<'_>> for SyscallMathOp {
    fn from(value: &Operator) -> Self {
        match value {
            Operator::F64ConvertI64U => SyscallMathOp::F64ConvertI64U,
            Operator::I32RemS => SyscallMathOp::I32RemS,
            Operator::I64RemS => SyscallMathOp::I64RemS,
            Operator::I32DivS => SyscallMathOp::I32DivS,
            Operator::I64DivS => SyscallMathOp::I64DivS,
            Operator::I32Clz => SyscallMathOp::I32Clz,
            Operator::I64Clz => SyscallMathOp::I64Clz,
            Operator::I32Ctz => SyscallMathOp::I32Ctz,
            Operator::I64Ctz => SyscallMathOp::I64Ctz,
            Operator::F64Add => SyscallMathOp::F64Add,
            Operator::F64Sub => SyscallMathOp::F64Sub,
            Operator::F64Mul => SyscallMathOp::F64Mul,
            Operator::F64Div => SyscallMathOp::F64Div,
            Operator::F64Eq => SyscallMathOp::F64Eq,
            Operator::F64Neg => SyscallMathOp::F64Neg,
            Operator::F64Ne => SyscallMathOp::F64Ne,
            Operator::F64Gt => SyscallMathOp::F64Gt,
            Operator::F64Ge => SyscallMathOp::F64Ge,
            Operator::F64Le => SyscallMathOp::F64Le,
            Operator::F64Lt => SyscallMathOp::F64Lt,
            Operator::F32Add => SyscallMathOp::F32Add,
            Operator::F32Sub => SyscallMathOp::F32Sub,
            Operator::F32Mul => SyscallMathOp::F32Mul,
            Operator::F32Div => SyscallMathOp::F32Div,
            Operator::F32Eq => SyscallMathOp::F32Eq,
            Operator::F32Neg => SyscallMathOp::F32Neg,
            Operator::F32Ne => SyscallMathOp::F32Ne,
            Operator::F32Gt => SyscallMathOp::F32Gt,
            Operator::F32Ge => SyscallMathOp::F32Ge,
            Operator::F32Le => SyscallMathOp::F32Le,
            Operator::F32Lt => SyscallMathOp::F32Lt,
            Operator::F64ConvertI32S => SyscallMathOp::F64ConvertI32S,
            Operator::F64ConvertI32U => SyscallMathOp::F64ConvertI32U,
            Operator::F64Abs => SyscallMathOp::F64Abs,
            Operator::F32Abs => SyscallMathOp::F32Abs,
            _ => unimplemented!(),
        }
    }
}
enum SyscallMemoryOp {
    I32Load = 1,
    I32Load16S,
    I32Load16U,
    I32Load8S,
    I32Load8U,
    I64Load,
    I64Load32S,
    I64Load32U,
    I64Load16S,
    I64Load16U,
    I64Load8S,
    I64Load8U,
    F64Load,
    F32Load,
    I32Store,
    I32Store16,
    I32Store8,
    I64Store,
    I64Store32,
    I64Store16,
    I64Store8,
    F32Store,
    F64Store,
    MemoryGrow,
    MemorySize,
}

impl Into<u64> for SyscallMemoryOp {
    fn into(self) -> u64 {
        self as u64
    }
}

impl From<&Operator<'_>> for SyscallMemoryOp {
    fn from(value: &Operator) -> Self {
        match value {
            Operator::I32Load { .. } => SyscallMemoryOp::I32Load,
            Operator::I32Load16S { .. } => SyscallMemoryOp::I32Load16S,
            Operator::I32Load16U { .. } => SyscallMemoryOp::I32Load16U,
            Operator::I32Load8S { .. } => SyscallMemoryOp::I32Load8S,
            Operator::I32Load8U { .. } => SyscallMemoryOp::I32Load8U,
            Operator::I64Load { .. } => SyscallMemoryOp::I64Load,
            Operator::I64Load32S { .. } => SyscallMemoryOp::I64Load32S,
            Operator::I64Load32U { .. } => SyscallMemoryOp::I64Load32U,
            Operator::I64Load16S { .. } => SyscallMemoryOp::I64Load16S,
            Operator::I64Load16U { .. } => SyscallMemoryOp::I64Load16U,
            Operator::I64Load8S { .. } => SyscallMemoryOp::I64Load8S,
            Operator::I64Load8U { .. } => SyscallMemoryOp::I64Load8U,
            Operator::F64Load { .. } => SyscallMemoryOp::F64Load,
            Operator::F32Load { .. } => SyscallMemoryOp::F32Load,

            Operator::I32Store { .. } => SyscallMemoryOp::I32Store,
            Operator::I32Store16 { .. } => SyscallMemoryOp::I32Store16,
            Operator::I32Store8 { .. } => SyscallMemoryOp::I32Store8,
            Operator::I64Store { .. } => SyscallMemoryOp::I64Store,
            Operator::I64Store32 { .. } => SyscallMemoryOp::I64Store32,
            Operator::I64Store16 { .. } => SyscallMemoryOp::I64Store16,
            Operator::I64Store8 { .. } => SyscallMemoryOp::I64Store8,
            Operator::F32Store { .. } => SyscallMemoryOp::F32Store,
            Operator::F64Store { .. } => SyscallMemoryOp::F64Store,

            Operator::MemoryGrow { .. } => SyscallMemoryOp::MemoryGrow,
            Operator::MemorySize { .. } => SyscallMemoryOp::MemorySize,
            _ => unimplemented!(),
        }
    }
}

#[derive(Debug)]
struct FnSignature {
    params: Vec<WasmType>,
    returns: Vec<WasmType>,
}

struct Function<'a> {
    function: FunctionValue<'a>,
    type_idx: TypeIndex,
    exportable: bool,
    imported: bool,
}

impl<'a> Function<'a> {
    fn set_exportable(&mut self) {
        self.exportable = true
    }

    fn is_exportable(&self) -> bool {
        self.exportable
    }

    fn is_imported(&self) -> bool {
        self.imported
    }
}

struct FnBody;

#[derive(Clone)]
enum StackValue<'a> {
    IntValue(IntValue<'a>),
    FloatValue(IntValue<'a>),
}

#[derive(Clone)]
struct ValueStack<'a> {
    stack: Vec<StackValue<'a>>,
}

impl<'a> ValueStack<'a> {
    fn push(&mut self, value: StackValue<'a>) {
        self.stack.push(value);
    }

    fn push_item(&mut self, item: StackValue<'a>) {
        self.stack.push(item);
    }

    fn pop(&mut self) -> Option<StackValue<'a>> {
        self.stack.pop().clone()
    }

    fn pop_int(&mut self) -> Result<IntValue<'a>, &str> {
        if let Some(value) = self.stack.pop() {
            let int_value = match value {
                StackValue::IntValue(v) => Ok(v),
                _ => Err("There is not Integer value on top of stack"),
            };
            int_value
        } else {
            Err("Stack is empty")
        }
    }

    fn pop_float(&mut self) -> Result<IntValue<'a>, &str> {
        if let Some(value) = self.stack.pop() {
            let int_value = match value {
                StackValue::FloatValue(v) => Ok(v),
                _ => Err("There is not Float value on top of stack"),
            };
            int_value
        } else {
            Err("Stack is empty")
        }
    }

    fn peek(&self, index: u32) -> Option<StackValue<'a>> {
        if index as usize + 1 > self.stack.len() {
            return None;
        }
        let idx = self.stack.len() - 1 - index as usize; // index from the top of stack
        self.stack.get(idx).cloned()
    }

    fn copy_stack(&self, count: usize) -> Option<Self> {
        if self.stack.len() < count {
            None
        } else {
            let idx = self.stack.len() - count;
            Some(Self {
                stack: self.stack[idx..].to_vec(),
            })
        }
    }
    fn drop(&mut self, count: usize) {
        for _ in 0..count {
            self.stack.pop();
        }
    }

    fn new() -> Self {
        Self { stack: Vec::new() }
    }
}

#[derive(Clone)]
enum ControlBlock<'a> {
    Loop {
        loop_start: BasicBlock<'a>,
        loop_end: BasicBlock<'a>,
        value_stack: ValueStack<'a>,
        params_count: usize,
        params: Vec<LocalVar<'a>>,
        results: Vec<LocalVar<'a>>,
        stack_polymorphic: bool,
    },
    IfElse {
        then: BasicBlock<'a>,
        if_else: BasicBlock<'a>,
        end: BasicBlock<'a>,
        else_is_found: bool,
        value_stack: ValueStack<'a>,
        params_count: usize,
        results: Vec<LocalVar<'a>>,
        stack_polymorphic: bool,
    },
    Block {
        block_end: BasicBlock<'a>,
        value_stack: ValueStack<'a>,
        params_count: usize,
        results: Vec<LocalVar<'a>>,
        stack_polymorphic: bool,
    },
    Function {
        entry: BasicBlock<'a>,
        end: BasicBlock<'a>,
        value_stack: ValueStack<'a>,
        results: Vec<LocalVar<'a>>,
        stack_polymorphic: bool,
    },
}

impl<'a> ControlBlock<'a> {
    fn get_end_basic_block(&self) -> BasicBlock<'a> {
        match self {
            Self::Loop { loop_start, .. } => *loop_start,
            Self::Block { block_end, .. } => *block_end,
            Self::IfElse { end, .. } => *end,
            Self::Function { end, .. } => *end,
        }
    }

    fn get_results(&self) -> Vec<LocalVar<'a>> {
        match self {
            Self::Loop { results, .. } => results.clone(),
            Self::Block { results, .. } => results.clone(),
            Self::IfElse { results, .. } => results.clone(),
            Self::Function { results, .. } => results.clone(),
        }
    }

    fn add_function_results(&mut self, results: Vec<LocalVar<'a>>) {
        let mut other = results.clone();
        match self {
            ControlBlock::Function { results, .. } => {
                results.append(&mut other);
            }
            _ => (),
        }
    }
}

struct ControlStack<'a> {
    vec: Vec<ControlBlock<'a>>,
    function: FunctionValue<'a>,
}

impl<'a> ControlStack<'a> {
    fn new(function: &FunctionValue<'a>) -> Self {
        Self {
            vec: Vec::new(),
            function: function.clone(),
        }
    }

    fn append_if_else(
        &mut self,
        llvm_context: &'a Context,
        llvm_builder: &'a Builder,
        params_count: usize,
        results: Vec<LocalVar<'a>>,
    ) {
        let current_block = llvm_builder.get_insert_block().unwrap();
        let then: BasicBlock = llvm_context.insert_basic_block_after(current_block, IF_THEN_NAME);
        let if_else = llvm_context.insert_basic_block_after(then, IF_ELSE_NAME);
        let end = llvm_context.insert_basic_block_after(if_else, IF_END_NAME);
        let new_stack = self
            .current_stack()
            .copy_stack(params_count)
            .expect("Can't copy value stack");
        self.vec.push(ControlBlock::IfElse {
            then,
            if_else,
            end,
            else_is_found: false,
            value_stack: new_stack.clone(),
            params_count,
            results,
            stack_polymorphic: false,
        })
    }

    fn append_loop(
        &mut self,
        llvm_context: &'a Context,
        llvm_builder: &'a Builder,
        params: Vec<LocalVar<'a>>,
        results: Vec<LocalVar<'a>>,
    ) {
        let params_count = params.len();
        let current_block = llvm_builder.get_insert_block().unwrap();
        let loop_start = llvm_context.insert_basic_block_after(current_block, LOOP_START_NAME);
        let loop_end = llvm_context.insert_basic_block_after(loop_start, LOOP_END_NAME);
        let new_stack = self
            .current_stack()
            .copy_stack(params_count)
            .expect("Can't copy value stack");
        self.vec.push(ControlBlock::Loop {
            loop_start,
            loop_end,
            value_stack: new_stack,
            params_count,
            params,
            results,
            stack_polymorphic: false,
        })
    }

    fn append_block(
        &mut self,
        llvm_context: &'a Context,
        llvm_builder: &'a Builder,
        params_count: usize,
        results: Vec<LocalVar<'a>>,
    ) {
        let current_block = llvm_builder.get_insert_block().unwrap();
        let block_end = llvm_context.insert_basic_block_after(current_block, BLOCK_END_NAME);
        let new_stack = self
            .current_stack()
            .copy_stack(params_count)
            .expect("Can't copy value stack");
        self.vec.push(ControlBlock::Block {
            block_end,
            value_stack: new_stack,
            params_count,
            results,
            stack_polymorphic: false,
        })
    }

    fn add_function_block(&mut self, llvm_context: &'a Context) {
        let block_start = llvm_context.append_basic_block(self.function, FUNCTION_ENTRY_NAME);
        let block_end = llvm_context.append_basic_block(self.function, FUNCTION_END_NAME);
        self.vec.push(ControlBlock::Function {
            entry: block_start,
            end: block_end,
            value_stack: ValueStack::new(),
            results: Vec::new(),
            stack_polymorphic: false,
        });
    }

    fn pop(&mut self) -> Option<ControlBlock<'a>> {
        self.vec.pop()
    }

    fn peek(&self, index: u32) -> Option<ControlBlock<'a>> {
        let idx = self.vec.len() - 1 - index as usize; // index from the top of stack
        self.vec.get(idx).cloned()
    }

    fn peek_mut(&mut self, index: u32) -> Option<&mut ControlBlock<'a>> {
        self.vec.iter_mut().rev().nth(index as usize)
    }

    fn top(&self) -> Option<ControlBlock<'a>> {
        self.peek(0)
    }

    fn top_mut(&mut self) -> Option<&mut ControlBlock<'a>> {
        self.peek_mut(0)
    }

    fn max_depth(&self) -> u32 {
        self.vec.len() as u32 - 1
    }

    fn current_stack(&mut self) -> &mut ValueStack<'a> {
        match self.vec.last_mut().unwrap() {
            ControlBlock::Loop { value_stack, .. } => value_stack,
            ControlBlock::Block { value_stack, .. } => value_stack,
            ControlBlock::IfElse { value_stack, .. } => value_stack,
            ControlBlock::Function { value_stack, .. } => value_stack,
        }
    }

    fn reinit_ifelse_value_stack(&mut self) {
        let old_block = self.pop().unwrap();
        match old_block {
            ControlBlock::IfElse {
                then,
                if_else,
                end,
                else_is_found,
                value_stack: _,
                params_count,
                results,
                stack_polymorphic: _,
            } => {
                let new_stack = self
                    .current_stack()
                    .copy_stack(params_count)
                    .expect("Can't copy value stack");
                self.vec.push(ControlBlock::IfElse {
                    then,
                    if_else,
                    end,
                    else_is_found,
                    value_stack: new_stack.clone(),
                    params_count,
                    results,
                    stack_polymorphic: false,
                })
            }
            _ => (),
        }
    }

    fn set_else_is_found(&mut self) {
        let block_ref = self.top_mut().unwrap();
        match block_ref {
            ControlBlock::IfElse { else_is_found, .. } => {
                *else_is_found = true;
            }
            _ => (),
        }
    }

    fn get_else_is_found(&self) -> bool {
        let block = self.top().unwrap();
        match block {
            ControlBlock::IfElse { else_is_found, .. } => else_is_found,
            _ => false,
        }
    }

    fn set_stack_polymorphic(&mut self) {
        let block_ref = self.top_mut().unwrap();
        match block_ref {
            ControlBlock::Loop {
                stack_polymorphic, ..
            } => *stack_polymorphic = true,
            ControlBlock::Block {
                stack_polymorphic, ..
            } => *stack_polymorphic = true,
            ControlBlock::IfElse {
                stack_polymorphic, ..
            } => *stack_polymorphic = true,
            ControlBlock::Function {
                stack_polymorphic, ..
            } => *stack_polymorphic = true,
        }
    }

    fn is_stack_polymorphic(&self) -> bool {
        let block_ref = self.top().unwrap();
        match block_ref {
            ControlBlock::Loop {
                stack_polymorphic, ..
            } => stack_polymorphic,
            ControlBlock::Block {
                stack_polymorphic, ..
            } => stack_polymorphic,
            ControlBlock::IfElse {
                stack_polymorphic, ..
            } => stack_polymorphic,
            ControlBlock::Function {
                stack_polymorphic, ..
            } => stack_polymorphic,
        }
    }
}

#[derive(Clone, Debug)]
struct LocalVar<'a> {
    pub ty: ValType,
    pub value: PointerValue<'a>,
}

#[derive(Clone, Debug)]
struct GlobalVar<'a> {
    global: Global,
    value: GlobalValue<'a>,
}

#[derive(Clone, Debug)]
struct TableVar<'a> {
    value: GlobalValue<'a>,
    array_ty: ArrayType<'a>,
    table: Table,
    original_data: Vec<FuncIndex>,
}

pub struct EnvironmentImpl<'a> {
    signatures: Vec<FnSignature>,
    function_names: HashMap<FuncIndex, String>,
    functions: Vec<Function<'a>>,
    function_bodies: Vec<FnBody>,
    imported_funcs: Vec<(String, String)>,
    exported_funcs: HashMap<FuncIndex, String>,
    global_variables: Vec<GlobalVar<'a>>,
    tables: Vec<TableVar<'a>>,
    math_op_syscall64: Option<FunctionValue<'a>>,
    math_op_syscall32: Option<FunctionValue<'a>>,
    memory_op_syscall64: Option<FunctionValue<'a>>,
    memory_op_syscall32: Option<FunctionValue<'a>>,
    fn_param_buffer: Option<GlobalValue<'a>>,

    llvm_context: &'a Context,
    llvm_builder: Builder<'a>,
    llvm_module: Module<'a>,
}

impl<'a> EnvironmentImpl<'a> {
    pub fn new(llvm_context: &'a Context) -> Self {
        Self {
            signatures: Vec::new(),
            function_names: HashMap::new(),
            functions: Vec::new(),
            function_bodies: Vec::new(),
            imported_funcs: Vec::new(),
            exported_funcs: HashMap::new(),
            global_variables: Vec::new(),
            tables: Vec::new(),
            math_op_syscall64: None,
            math_op_syscall32: None,
            memory_op_syscall64: None,
            memory_op_syscall32: None,
            fn_param_buffer: None,
            llvm_context,
            llvm_builder: llvm_context.create_builder(),
            llvm_module: llvm_context.create_module("Unnamed_module"),
        }
    }
}

impl<'data> ModuleEnvironment<'data> for EnvironmentImpl<'data> {
    fn declare_type_func(&mut self, wasm: WasmFuncType) -> WasmResult<()> {
        // let mut sig = ir::Signature::new(CallConv::Fast);
        // let mut cvt = |ty: &WasmType| {
        //     let reference_type = match self.pointer_type() {
        //         ir::types::I32 => ir::types::R32,
        //         ir::types::I64 => ir::types::R64,
        //         _ => panic!("unsupported pointer type"),
        //     };
        //     ir::AbiParam::new(match ty {
        //         WasmType::I32 => ir::types::I32,
        //         WasmType::I64 => ir::types::I64,
        //         WasmType::F32 => ir::types::F32,
        //         WasmType::F64 => ir::types::F64,
        //         WasmType::V128 => ir::types::I8X16,
        //         WasmType::FuncRef | WasmType::ExternRef => reference_type,
        //     })
        // };
        // sig.params.extend(wasm.params().iter().map(&mut cvt));
        // sig.returns.extend(wasm.returns().iter().map(&mut cvt));
        // self.info.signatures.push(sig);
        self.signatures.push(FnSignature {
            params: Vec::from(wasm.params()),
            returns: Vec::from(wasm.returns()),
        });
        debug!("FUNC {:?} {:?}", self.signatures.len() - 1, wasm);
        Ok(())
    }

    fn declare_func_import(
        &mut self,
        index: TypeIndex,
        module: &'data str,
        field: &'data str,
    ) -> WasmResult<()> {
        // assert_eq!(
        //     self.info.functions.len(),
        //     self.info.imported_funcs.len(),
        //     "Imported functions must be declared first"
        // );
        // self.info.functions.push(Exportable::new(index));
        // self.info
        //     .imported_funcs
        //     .push((String::from(module), String::from(field)));
        let fn_name = format!("{module}_{field}");

        self.imported_funcs
            .push((String::from(module), String::from(field)));
        self.add_function(index, Some(fn_name), true);

        Ok(())
    }

    fn declare_func_type(&mut self, index: TypeIndex) -> WasmResult<()> {
        self.add_function(index, None, false);
        Ok(())
    }

    fn declare_global(&mut self, global: Global) -> WasmResult<()> {
        let address_space: Option<inkwell::AddressSpace> = None;
        let value = match global.initializer {
            GlobalInit::I32Const(val) => self.llvm_context.i32_type().const_int(val as u64, false),
            GlobalInit::I64Const(val) => self.llvm_context.i64_type().const_int(val as u64, false),
            GlobalInit::F32Const(val) => self.llvm_context.i32_type().const_int(val as u64, false),
            GlobalInit::F64Const(val) => self.llvm_context.i64_type().const_int(val as u64, false),
            _ => unimplemented!(),
        };

        let global_var =
            self.llvm_module
                .add_global(value.get_type(), address_space, GLOBAL_VAR_NAME);
        global_var.set_initializer(&value);
        global_var.set_linkage(Linkage::Internal);
        // TODO: Uncomment when the eBPF machine will be able to relocate .rodata
        // global_var.set_constant(!global.mutability);

        self.global_variables.push(GlobalVar {
            global,
            value: global_var,
        });

        Ok(())
    }

    fn declare_global_import(
        &mut self,
        _global: Global,
        _module: &'data str,
        _field: &'data str,
    ) -> WasmResult<()> {
        // self.info.globals.push(Exportable::new(global));
        // self.info
        //     .imported_globals
        //     .push((String::from(module), String::from(field)));
        unimplemented!();
    }

    fn declare_table(&mut self, table: Table) -> WasmResult<()> {
        let len = if let Some(max) = table.maximum {
            max
        } else {
            table.minimum
        };

        let llvm_ty = match table.wasm_ty {
            WasmType::FuncRef => self.llvm_context.i32_type(),
            _ => unimplemented!(),
        };

        let array_ty = llvm_ty.array_type(len);
        let global_var = self.llvm_module.add_global(array_ty, None, TABLE_VAR_NAME);

        // Init the memory with zeros
        let zeros = (0..len)
            .map(|_| llvm_ty.const_zero())
            .collect::<Vec<IntValue>>();
        let zeros_array = llvm_ty.const_array(&zeros);

        global_var.set_initializer(&zeros_array);

        self.tables.push(TableVar {
            value: global_var,
            array_ty,
            table,
            original_data: vec![FuncIndex::from_u32(0); len as usize],
        });

        Ok(())
    }

    fn declare_table_import(
        &mut self,
        _table: Table,
        _module: &'data str,
        _field: &'data str,
    ) -> WasmResult<()> {
        // self.info.tables.push(Exportable::new(table));
        // self.info
        //     .imported_tables
        //     .push((String::from(module), String::from(field)));
        unimplemented!()
    }

    fn declare_table_elements(
        &mut self,
        table_index: TableIndex,
        base: Option<GlobalIndex>,
        offset: u32,
        elements: Box<[FuncIndex]>,
    ) -> WasmResult<()> {
        if base.is_some() {
            // What `Global index` is? Need to initialize the given global variable with the given data?
            todo!("What `Global index` is?");
        }
        let offset = offset as usize;
        if let Some(table) = self.tables.get_mut(table_index.as_u32() as usize) {
            if table
                .original_data
                .get(elements.len() + offset - 1)
                .is_none()
            {
                return Err(WasmError::User(format!(
                    "Can't init table idx={} by offset={} because there are too many elemnents {}",
                    table_index.as_u32(),
                    offset,
                    elements.len()
                )));
            }

            // Update content
            table
                .original_data
                .splice(offset.., elements.iter().cloned());

            // Re-generate the initializer again
            let element_ty = table.array_ty.get_element_type().into_int_type();
            let elems = table
                .original_data
                .iter()
                .map(|e| element_ty.const_int(e.as_u32() as u64, false))
                .collect::<Vec<_>>();

            let array = element_ty.const_array(&elems);
            table.value.set_initializer(&array);

            Ok(())
        } else {
            Err(WasmError::User(format!(
                "Can't table by idx={}",
                table_index.as_u32()
            )))
        }
    }

    fn declare_passive_element(
        &mut self,
        _elem_index: ElemIndex,
        _segments: Box<[FuncIndex]>,
    ) -> WasmResult<()> {
        unimplemented!()
    }

    fn declare_passive_data(
        &mut self,
        _elem_index: DataIndex,
        _segments: &'data [u8],
    ) -> WasmResult<()> {
        unimplemented!()
    }

    fn declare_memory(&mut self, memory: Memory) -> WasmResult<()> {
        let i64_type = self.llvm_context.i64_type();

        // struct {
        //    minimum: u64,
        //    maximum: u64,
        // }
        let memory_struct_type = self
            .llvm_context
            .struct_type(&[i64_type.into(), i64_type.into()], true);
        let memory_struct_value = memory_struct_type.const_named_struct(&[
            i64_type.const_int(memory.minimum, false).into(),
            i64_type
                .const_int(memory.maximum.unwrap_or(0), false)
                .into(),
        ]);

        let global_var =
            self.llvm_module
                .add_global(memory_struct_value.get_type(), None, MEMORY_VAR_NAME);

        global_var.set_section(Some(MEMORY_SECTION_NAME));
        global_var.set_alignment(1);
        global_var.set_initializer(&memory_struct_value);

        Ok(())
    }

    fn declare_memory_import(
        &mut self,
        _memory: Memory,
        _module: &'data str,
        _field: &'data str,
    ) -> WasmResult<()> {
        // self.info.memories.push(Exportable::new(memory));
        // self.info
        //     .imported_memories
        //     .push((String::from(module), String::from(field)));
        // Ok(())
        unimplemented!();
    }

    fn declare_data_initialization(
        &mut self,
        memory_index: MemoryIndex,
        base: Option<GlobalIndex>,
        offset: u64,
        data: &'data [u8],
    ) -> WasmResult<()> {
        if base.is_some() {
            // What `Global index` is? Need to initialize the given global variable with the given data?
            todo!("What `Global index` is?");
        }

        let i32_type = self.llvm_context.i32_type();
        let i64_type = self.llvm_context.i64_type();
        let i8_type = self.llvm_context.i8_type();

        // struct {
        //    memory_index: u32,
        //    offset: u64,
        //    data_len: u64,
        //    data: [u8]
        // }
        let init_mem_struct_type = self.llvm_context.struct_type(
            &[
                i32_type.into(),
                i64_type.into(),
                i64_type.into(),
                i8_type.array_type(data.len() as _).into(),
            ],
            true,
        );
        let elements = data
            .iter()
            .map(|v| self.llvm_context.i8_type().const_int(*v as u64, false))
            .collect::<Vec<_>>();
        let init_mem_struct_value = init_mem_struct_type.const_named_struct(&[
            i32_type.const_int(memory_index.as_u32() as _, false).into(),
            i64_type.const_int(offset, false).into(),
            i64_type.const_int(data.len() as _, false).into(),
            i8_type.const_array(&elements).into(),
        ]);

        let global_var = self.llvm_module.add_global(
            init_mem_struct_value.get_type(),
            None,
            INIT_MEMORY_VAR_NAME,
        );

        global_var.set_section(Some(INIT_MEMORY_SECTION_NAME));
        global_var.set_alignment(1);
        global_var.set_initializer(&init_mem_struct_value);

        Ok(())
    }

    fn declare_func_export(&mut self, func_index: FuncIndex, name: &'data str) -> WasmResult<()> {
        // self.info.functions[func_index]
        //     .export_names
        //     .push(String::from(name));
        let function = self
            .functions
            .get_mut(func_index.as_u32() as usize)
            .expect("Can't find a function");

        self.exported_funcs.insert(func_index, String::from(name));
        self.function_names.insert(func_index, String::from(name));

        function.function.as_global_value().set_name(name);
        function.set_exportable();
        function.function.set_linkage(Linkage::External);

        Ok(())
    }

    fn declare_table_export(
        &mut self,
        _table_index: TableIndex,
        _name: &'data str,
    ) -> WasmResult<()> {
        // self.info.tables[table_index]
        //     .export_names
        //     .push(String::from(name));
        unimplemented!()
    }

    fn declare_memory_export(
        &mut self,
        _memory_index: MemoryIndex,
        _name: &'data str,
    ) -> WasmResult<()> {
        // self.info.memories[memory_index]
        //     .export_names
        //     .push(String::from(name));
        Ok(())
    }

    fn declare_global_export(
        &mut self,
        _global_index: GlobalIndex,
        _name: &'data str,
    ) -> WasmResult<()> {
        // self.info.globals[global_index]
        //     .export_names
        //     .push(String::from(name));
        Ok(())
    }

    fn declare_start_func(&mut self, _func_index: FuncIndex) -> WasmResult<()> {
        // debug_assert!(self.info.start_func.is_none());
        // self.info.start_func = Some(func_index);
        unimplemented!("{:?}", _func_index);
    }

    fn define_function_body(
        &mut self,
        validator: FuncValidator<ValidatorResources>,
        body: FunctionBody<'data>,
    ) -> WasmResult<()> {
        // self.func_bytecode_sizes
        //     .push(body.get_binary_reader().bytes_remaining());
        // let func = {
        //     let mut func_environ =
        //         DummyFuncEnvironment::new(&self.info, self.expected_reachability.clone());
        //     let func_index =
        //         FuncIndex::new(self.get_num_func_imports() + self.info.function_bodies.len());

        //     let sig = func_environ.vmctx_sig(self.get_func_type(func_index));
        //     let mut func =
        //         ir::Function::with_name_signature(UserFuncName::user(0, func_index.as_u32()), sig);

        //     if self.debug_info {
        //         func.collect_debug_info();
        //     }

        //     self.trans
        //         .translate_body(&mut validator, body, &mut func, &mut func_environ)?;
        //     func
        // };
        // self.info.function_bodies.push(func);

        self.add_fn_param_buffer();
        self.add_math_op_syscalls();
        self.add_memory_op_syscalls();
        self.translate_function(validator, body)?;
        self.function_bodies.push(FnBody {});

        Ok(())
    }

    fn declare_module_name(&mut self, name: &'data str) {
        self.llvm_module.set_name(name);
    }

    fn declare_func_name(&mut self, func_index: FuncIndex, name: &'data str) {
        let function = self
            .functions
            .get(func_index.as_u32() as usize)
            .expect("Can't find a function");

        self.function_names.insert(func_index, String::from(name));
        if !function.is_exportable() && !function.is_imported() {
            function.function.as_global_value().set_name(name);
        }
    }

    fn wasm_features(&self) -> cranelift_wasm::wasmparser::WasmFeatures {
        cranelift_wasm::wasmparser::WasmFeatures {
            multi_value: true,
            simd: true,
            reference_types: true,
            bulk_memory: true,
            ..cranelift_wasm::wasmparser::WasmFeatures::default()
        }
    }
}

impl<'a> EnvironmentImpl<'a> {
    pub fn print_to_stderr(&self) {
        self.llvm_module.print_to_stderr();
    }

    pub fn print_to_file(&self, path: &PathBuf) -> Result<(), anyhow::Error> {
        self.llvm_module
            .print_to_file(path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    pub fn print_to_string(&self) -> String {
        let llvm_string = self.llvm_module.print_to_string();
        llvm_string.to_string()
    }

    pub fn write_bitcode_to_memory(&self) -> MemoryBuffer {
        match self.llvm_module.verify() {
            Ok(_) => (),
            Err(e) => {
                self.print_to_stderr();
                panic!("{:?}", e);
            }
        };
        self.llvm_module.write_bitcode_to_memory()
    }

    fn translate_function(
        &mut self,
        mut validator: FuncValidator<ValidatorResources>,
        body: FunctionBody,
    ) -> WasmResult<()> {
        let llvm_context = self.llvm_context;
        let llvm_module = &self.llvm_module;
        let llvm_builder = &self.llvm_builder;

        let mut reader = body.get_binary_reader();
        let local_count = reader.read_var_u32()?;
        let func_index =
            FuncIndex::from_u32((self.imported_funcs.len() + self.function_bodies.len()) as u32);

        let (llvm_function, fn_signature) = {
            let function = self
                .functions
                .get(func_index.as_u32() as usize)
                .expect("Can't find a function");

            (
                function.function,
                &self.signatures[function.type_idx.as_u32() as usize],
            )
        };

        debug!("FN_INDEX: {func_index:?} SIG: {fn_signature:?}");

        let mut ctrl_stack = ControlStack::new(&llvm_function);
        ctrl_stack.add_function_block(&llvm_context);
        let local_var_builder = LocalVarBuilder::new(
            llvm_function.get_first_basic_block().unwrap(),
            llvm_context.i64_type(),
        );
        {
            if let Some(ControlBlock::Function { entry, .. }) = ctrl_stack.top() {
                llvm_builder.position_at_end(entry);
            }

            // Generate function results
            let mut fn_results = Vec::<LocalVar>::new();
            for wasm_ty in fn_signature.returns.iter() {
                let ptr = local_var_builder.get(llvm_builder, FUNCTION_RESULT_VAR_NAME);
                fn_results.push(LocalVar {
                    ty: wasm_ty.clone().into(),
                    value: ptr,
                });
            }

            assert!(fn_results.len() < 2, "TODO: Function can't return an array");

            ctrl_stack
                .top_mut()
                .unwrap()
                .add_function_results(fn_results)
        }

        // {
        //     let bb = llvm_context.append_basic_block(llvm_function, "Body");
        //     llvm_builder.build_unconditional_branch(bb);

        //     llvm_builder.position_at_end(bb);
        // }

        let mut locals: Vec<LocalVar> = {
            let params =
                self.generate_function_prolog(&llvm_function, fn_signature, &local_var_builder);
            params
        };

        for local_n in 0..local_count as usize {
            let pos = reader.original_position();
            let count = reader.read_var_u32()?;
            let ty = reader.read::<ValType>()?;
            validator.define_locals(pos, count, ty)?;
            debug!(
                "FN SIGN: {:?} FN_BODY: {:?} LOCAL {:?}: {:?} {:?}",
                (&fn_signature.params, &fn_signature.returns),
                func_index,
                local_n,
                count,
                ty
            );
            for _ in 0..count {
                let value = local_var_builder.get(llvm_builder, FUNCTION_LOCAL_VAR_NAME);
                locals.push(LocalVar { ty, value });
            }
        }

        while !reader.eof() {
            let pos = reader.original_position();
            let op = reader.read_operator()?;
            validator.op(pos, &op)?;
            // if ctrl_stack.is_stack_polymorphic() {
            //    if !matches!(op, Operator::End | Operator::Else) {
            //     continue;
            //    }
            // }
            match op {
                Operator::I32Const { value } => {
                    let int_value = llvm_context.i32_type().const_int(value as u64, false);
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(int_value));
                }
                Operator::I64Const { value } => {
                    let int_value = llvm_context.i64_type().const_int(value as u64, false);
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(int_value));
                }
                Operator::F32Const { value } => {
                    let int_value = llvm_context
                        .i32_type()
                        .const_int(value.bits() as u64, false);
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(int_value));
                }
                Operator::F64Const { value } => {
                    let int_value = llvm_context.i64_type().const_int(value.bits(), false);
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(int_value));
                }
                Operator::LocalSet { local_index } | Operator::LocalTee { local_index } => {
                    let value = ctrl_stack.current_stack().pop().expect("Stack error");
                    let var = locals
                        .get(local_index as usize)
                        .expect(&format!("Local var error {}", local_index));
                    match value {
                        StackValue::IntValue(val) => llvm_builder.build_store(var.value, val),
                        StackValue::FloatValue(val) => llvm_builder.build_store(var.value, val),
                    };
                    if matches!(op, Operator::LocalTee { local_index: _ }) {
                        ctrl_stack.current_stack().push_item(value);
                    }
                }
                Operator::LocalGet { local_index } => {
                    let var = locals.get(local_index as usize).expect("Local var error");
                    let pointee_ty = get_llvm_type(llvm_context, var.ty).as_basic_type_enum();
                    let load_value =
                        llvm_builder.build_load(pointee_ty, var.value, FUNCTION_TMP_VAR_NAME);
                    match var.ty {
                        ValType::I32 | ValType::I64 => ctrl_stack
                            .current_stack()
                            .push(StackValue::IntValue(load_value.into_int_value())),
                        ValType::F32 | ValType::F64 => ctrl_stack
                            .current_stack()
                            .push(StackValue::FloatValue(load_value.into_int_value())),
                        _ => unreachable!(),
                    }
                }
                Operator::GlobalGet { global_index } => {
                    let global_var = self
                        .global_variables
                        .get(global_index as usize)
                        .expect("Global var error");
                    let ptr = global_var.value.as_pointer_value();
                    let pointee_ty =
                        get_llvm_type(llvm_context, global_var.global.wasm_ty.clone().into())
                            .as_basic_type_enum();
                    let load_value =
                        llvm_builder.build_load(pointee_ty, ptr, FUNCTION_TMP_VAR_NAME);

                    let value = match global_var.global.wasm_ty {
                        WasmType::I32 | WasmType::I64 => {
                            StackValue::IntValue(load_value.into_int_value())
                        }
                        WasmType::F32 | WasmType::F64 => {
                            StackValue::FloatValue(load_value.into_int_value())
                        }
                        _ => unimplemented!(),
                    };
                    ctrl_stack.current_stack().push(value);
                }
                Operator::GlobalSet { global_index } => {
                    let value = ctrl_stack.current_stack().pop().expect("Stack error");
                    let global_var = self
                        .global_variables
                        .get(global_index as usize)
                        .expect("Global var error");

                    let ptr = global_var.value.as_pointer_value();
                    match value {
                        StackValue::IntValue(val) => llvm_builder.build_store(ptr, val),
                        StackValue::FloatValue(val) => llvm_builder.build_store(ptr, val),
                    };
                }
                Operator::I32Add
                | Operator::I64Add
                | Operator::I32Sub
                | Operator::I64Sub
                | Operator::I32Mul
                | Operator::I64Mul
                | Operator::I32DivS
                | Operator::I64DivS
                | Operator::I32DivU
                | Operator::I64DivU
                | Operator::I32RemS
                | Operator::I64RemS
                | Operator::I32RemU
                | Operator::I64RemU
                | Operator::I32And
                | Operator::I64And
                | Operator::I32Or
                | Operator::I64Or
                | Operator::I32Xor
                | Operator::I64Xor
                | Operator::I32Shl
                | Operator::I64Shl
                | Operator::I32ShrS
                | Operator::I64ShrS
                | Operator::I32ShrU
                | Operator::I64ShrU => {
                    let rhs = ctrl_stack.current_stack().pop().expect("Stack error");
                    let lhs = ctrl_stack.current_stack().pop().expect("Stack error");

                    match lhs {
                        StackValue::IntValue(lval) => match rhs {
                            StackValue::IntValue(rval) => {
                                let int_value = match op {
                                    Operator::I32Add | Operator::I64Add => llvm_builder
                                        .build_int_add(lval, rval, FUNCTION_TMP_VAR_NAME),
                                    Operator::I32Sub | Operator::I64Sub => llvm_builder
                                        .build_int_sub(lval, rval, FUNCTION_TMP_VAR_NAME),
                                    Operator::I32Mul | Operator::I64Mul => llvm_builder
                                        .build_int_mul(lval, rval, FUNCTION_TMP_VAR_NAME),
                                    Operator::I32DivS
                                    | Operator::I64DivS
                                    | Operator::I32RemS
                                    | Operator::I64RemS => {
                                        if matches!(op, Operator::I32DivS | Operator::I32RemS) {
                                            let args = [
                                                llvm_context
                                                    .i32_type()
                                                    .const_int(
                                                        SyscallMathOp::from(&op).into(),
                                                        false,
                                                    )
                                                    .into(),
                                                lval.into(),
                                                rval.into(),
                                            ];
                                            self.build_call_math_op_syscall32(
                                                &args,
                                                FUNCTION_TMP_VAR_NAME,
                                            )
                                        } else {
                                            let args = [
                                                llvm_context
                                                    .i64_type()
                                                    .const_int(
                                                        SyscallMathOp::from(&op).into(),
                                                        false,
                                                    )
                                                    .into(),
                                                lval.into(),
                                                rval.into(),
                                            ];
                                            self.build_call_math_op_syscall64(
                                                &args,
                                                FUNCTION_TMP_VAR_NAME,
                                            )
                                        }
                                    }
                                    Operator::I32DivU | Operator::I64DivU => llvm_builder
                                        .build_int_unsigned_div(lval, rval, FUNCTION_TMP_VAR_NAME),
                                    Operator::I32RemU | Operator::I64RemU => llvm_builder
                                        .build_int_unsigned_rem(lval, rval, FUNCTION_TMP_VAR_NAME),
                                    Operator::I32And | Operator::I64And => {
                                        llvm_builder.build_and(lval, rval, FUNCTION_TMP_VAR_NAME)
                                    }
                                    Operator::I32Or | Operator::I64Or => {
                                        llvm_builder.build_or(lval, rval, FUNCTION_TMP_VAR_NAME)
                                    }
                                    Operator::I32Xor | Operator::I64Xor => {
                                        llvm_builder.build_xor(lval, rval, FUNCTION_TMP_VAR_NAME)
                                    }
                                    Operator::I32Shl | Operator::I64Shl => {
                                        let bit_width = rval.get_type().const_int(
                                            rval.get_type().get_bit_width() as u64,
                                            false,
                                        );
                                        let shift = llvm_builder.build_int_unsigned_rem(
                                            rval,
                                            bit_width,
                                            FUNCTION_TMP_VAR_NAME,
                                        );
                                        llvm_builder.build_left_shift(
                                            lval,
                                            shift,
                                            FUNCTION_TMP_VAR_NAME,
                                        )
                                    }

                                    Operator::I32ShrS | Operator::I64ShrS => {
                                        let bit_width = rval.get_type().const_int(
                                            rval.get_type().get_bit_width() as u64,
                                            false,
                                        );
                                        let shift = llvm_builder.build_int_unsigned_rem(
                                            rval,
                                            bit_width,
                                            FUNCTION_TMP_VAR_NAME,
                                        );
                                        llvm_builder.build_right_shift(
                                            lval,
                                            shift,
                                            true,
                                            FUNCTION_TMP_VAR_NAME,
                                        )
                                    }
                                    Operator::I32ShrU | Operator::I64ShrU => {
                                        let bit_width = rval.get_type().const_int(
                                            rval.get_type().get_bit_width() as u64,
                                            false,
                                        );
                                        let shift = llvm_builder.build_int_unsigned_rem(
                                            rval,
                                            bit_width,
                                            FUNCTION_TMP_VAR_NAME,
                                        );
                                        llvm_builder.build_right_shift(
                                            lval,
                                            shift,
                                            false,
                                            FUNCTION_TMP_VAR_NAME,
                                        )
                                    }
                                    _ => unreachable!(),
                                };
                                ctrl_stack
                                    .current_stack()
                                    .push(StackValue::IntValue(int_value));
                            }
                            _ => todo!(),
                        },
                        _ => todo!(),
                    };
                }
                Operator::I32Clz | Operator::I64Clz | Operator::I32Ctz | Operator::I64Ctz => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let value = if matches!(op, Operator::I32Clz | Operator::I32Ctz) {
                        let args = [
                            llvm_context
                                .i32_type()
                                .const_int(SyscallMathOp::from(&op).into(), false)
                                .into(),
                            int_value.into(),
                            llvm_context.i32_type().const_zero().into(),
                        ];
                        self.build_call_math_op_syscall32(&args, FUNCTION_TMP_VAR_NAME)
                    } else {
                        let args = [
                            llvm_context
                                .i64_type()
                                .const_int(SyscallMathOp::from(&op).into(), false)
                                .into(),
                            int_value.into(),
                            llvm_context.i64_type().const_zero().into(),
                        ];
                        self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME)
                    };

                    ctrl_stack.current_stack().push(StackValue::IntValue(value));
                }
                Operator::I32Rotl | Operator::I64Rotl | Operator::I32Rotr | Operator::I64Rotr => {
                    let rhs = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let lhs = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let bit_width = rhs.get_type().get_bit_width();
                    let bitw_value = rhs.get_type().const_int(bit_width as u64, false);
                    let shift =
                        llvm_builder.build_int_unsigned_rem(rhs, bitw_value, FUNCTION_TMP_VAR_NAME);
                    match op {
                        Operator::I32Rotl | Operator::I64Rotl => {
                            /* Rotl
                            %a = sub i32 32, %rhs
                            %b = shl i32 %lhs, %rhs
                            %c = lshr i32 %lhs, %a
                            %d = or i32 %b, %c
                            result i32 %d
                            */
                            let a = llvm_builder.build_int_sub(
                                bitw_value,
                                shift,
                                FUNCTION_TMP_VAR_NAME,
                            );
                            let b =
                                llvm_builder.build_left_shift(lhs, shift, FUNCTION_TMP_VAR_NAME);
                            let c = llvm_builder.build_right_shift(
                                lhs,
                                a,
                                false,
                                FUNCTION_TMP_VAR_NAME,
                            );
                            let d = llvm_builder.build_or(b, c, FUNCTION_TMP_VAR_NAME);

                            ctrl_stack.current_stack().push(StackValue::IntValue(d));
                        }
                        Operator::I32Rotr | Operator::I64Rotr => {
                            /* Rotr
                            %a = sub i32 32, %rhs
                            %b = lshr i32 %lhs, %rhs
                            %c = shl i32 %lhs, %a
                            %d = or i32 %b, %c
                            result i32 %d
                            */
                            let a = llvm_builder.build_int_sub(
                                bitw_value,
                                shift,
                                FUNCTION_TMP_VAR_NAME,
                            );
                            let b = llvm_builder.build_right_shift(
                                lhs,
                                shift,
                                false,
                                FUNCTION_TMP_VAR_NAME,
                            );
                            let c = llvm_builder.build_left_shift(lhs, a, FUNCTION_TMP_VAR_NAME);
                            let d = llvm_builder.build_or(b, c, FUNCTION_TMP_VAR_NAME);

                            ctrl_stack.current_stack().push(StackValue::IntValue(d));
                        }
                        _ => unreachable!(),
                    }
                }
                Operator::I32Popcnt | Operator::I64Popcnt => {
                    // https://llvm.org/docs/LangRef.html#llvm-ctpop-intrinsic
                    let intristic =
                        Intrinsic::find("llvm.ctpop").expect("Can't find llvm.ctpop intristic");

                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let intristic_decl = intristic
                        .get_declaration(llvm_module, &[int_value.get_type().into()])
                        .expect("Can't find ctlz declaration");

                    let site_value = llvm_builder.build_call(
                        intristic_decl,
                        &[int_value.into()],
                        FUNCTION_TMP_VAR_NAME,
                    );
                    let ct_value = site_value
                        .try_as_basic_value()
                        .expect_left("Can't get a intristic's return value");

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(ct_value.into_int_value()));
                }
                Operator::I32Eq
                | Operator::I32Ne
                | Operator::I32LtS
                | Operator::I32LtU
                | Operator::I32GtS
                | Operator::I32GtU
                | Operator::I32LeS
                | Operator::I32LeU
                | Operator::I32GeS
                | Operator::I32GeU
                | Operator::I32Eqz
                | Operator::I64Eq
                | Operator::I64Ne
                | Operator::I64LtS
                | Operator::I64LtU
                | Operator::I64GtS
                | Operator::I64GtU
                | Operator::I64LeS
                | Operator::I64LeU
                | Operator::I64GeS
                | Operator::I64GeU
                | Operator::I64Eqz => {
                    let rhs = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let lhs = if matches!(op, Operator::I32Eqz | Operator::I64Eqz) {
                        rhs.get_type().const_zero()
                    } else {
                        ctrl_stack.current_stack().pop_int().expect("Stack error")
                    };
                    let predic = match op {
                        Operator::I32Eqz | Operator::I64Eqz | Operator::I32Eq | Operator::I64Eq => {
                            IntPredicate::EQ
                        }
                        Operator::I32Ne | Operator::I64Ne => IntPredicate::NE,
                        Operator::I32LtS | Operator::I64LtS => IntPredicate::SLT,
                        Operator::I32LtU | Operator::I64LtU => IntPredicate::ULT,
                        Operator::I32GtS | Operator::I64GtS => IntPredicate::SGT,
                        Operator::I32GtU | Operator::I64GtU => IntPredicate::UGT,
                        Operator::I32LeS | Operator::I64LeS => IntPredicate::SLE,
                        Operator::I32LeU | Operator::I64LeU => IntPredicate::ULE,
                        Operator::I32GeS | Operator::I64GeS => IntPredicate::SGE,
                        Operator::I32GeU | Operator::I64GeU => IntPredicate::UGE,
                        _ => unreachable!(),
                    };
                    let bool_value =
                        llvm_builder.build_int_compare(predic, lhs, rhs, FUNCTION_TMP_VAR_NAME);
                    // LLVM returns a boolean value but WASM always a returns i32 value
                    let int32_type = llvm_context.i32_type();
                    let int_value = llvm_builder.build_int_z_extend(
                        bool_value,
                        int32_type,
                        FUNCTION_TMP_VAR_NAME,
                    );

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(int_value));
                }
                Operator::F64Add
                | Operator::F64Sub
                | Operator::F64Div
                | Operator::F64Mul
                | Operator::F64Eq
                | Operator::F64Ne
                | Operator::F64Gt
                | Operator::F64Ge
                | Operator::F64Le
                | Operator::F64Lt => {
                    let rhs = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let lhs = ctrl_stack.current_stack().pop_float().expect("Stack error");

                    let i64type = llvm_context.i64_type();
                    let float_op = i64type.const_int(SyscallMathOp::from(&op).into(), false);

                    let args = [float_op.into(), lhs.into(), rhs.into()];
                    let ret_value = self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);
                    match op {
                        Operator::F64Add
                        | Operator::F64Sub
                        | Operator::F64Div
                        | Operator::F64Mul => ctrl_stack
                            .current_stack()
                            .push(StackValue::FloatValue(ret_value)),
                        Operator::F64Eq
                        | Operator::F64Ne
                        | Operator::F64Gt
                        | Operator::F64Ge
                        | Operator::F64Le
                        | Operator::F64Lt => {
                            // WASM instruction returns i32 so need to truncate i64 value to i32
                            let i32type = llvm_context.i32_type();
                            let ret_value = llvm_builder.build_int_truncate(
                                ret_value,
                                i32type,
                                FUNCTION_TMP_VAR_NAME,
                            );
                            ctrl_stack
                                .current_stack()
                                .push(StackValue::IntValue(ret_value))
                        }
                        _ => unreachable!(),
                    };
                }
                Operator::F32Add
                | Operator::F32Sub
                | Operator::F32Div
                | Operator::F32Mul
                | Operator::F32Eq
                | Operator::F32Ne
                | Operator::F32Gt
                | Operator::F32Ge
                | Operator::F32Le
                | Operator::F32Lt => {
                    let rhs = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let lhs = ctrl_stack.current_stack().pop_float().expect("Stack error");

                    let i32type = llvm_context.i32_type();
                    let float_op = i32type.const_int(SyscallMathOp::from(&op).into(), false);

                    let args = [float_op.into(), lhs.into(), rhs.into()];
                    let ret_value = self.build_call_math_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);
                    match op {
                        Operator::F32Add
                        | Operator::F32Sub
                        | Operator::F32Div
                        | Operator::F32Mul => ctrl_stack
                            .current_stack()
                            .push(StackValue::FloatValue(ret_value)),
                        Operator::F32Eq
                        | Operator::F32Ne
                        | Operator::F32Gt
                        | Operator::F32Ge
                        | Operator::F32Le
                        | Operator::F32Lt => ctrl_stack
                            .current_stack()
                            .push(StackValue::IntValue(ret_value)),
                        _ => unreachable!(),
                    }
                }
                Operator::F32Neg => {
                    let value = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let float_op = llvm_context
                        .i32_type()
                        .const_int(SyscallMathOp::F32Neg.into(), false);
                    let zero = llvm_context.i32_type().const_zero();
                    let args = [float_op.into(), value.into(), zero.into()];
                    let ret_value = self.build_call_math_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(ret_value));
                }
                Operator::F64Neg => {
                    let value = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let float_op = llvm_context
                        .i64_type()
                        .const_int(SyscallMathOp::F64Neg.into(), false);
                    let zero = llvm_context.i64_type().const_zero();
                    let args = [float_op.into(), value.into(), zero.into()];
                    let ret_value = self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(ret_value));
                }
                Operator::F32Abs => {
                    let value = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let float_op = llvm_context
                        .i32_type()
                        .const_int(SyscallMathOp::F32Abs.into(), false);
                    let zero = llvm_context.i32_type().const_zero();
                    let args = [float_op.into(), value.into(), zero.into()];
                    let ret_value = self.build_call_math_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(ret_value));
                }
                Operator::F64Abs => {
                    let value = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    let float_op = llvm_context
                        .i64_type()
                        .const_int(SyscallMathOp::F64Abs.into(), false);
                    let zero = llvm_context.i64_type().const_zero();
                    let args = [float_op.into(), value.into(), zero.into()];
                    let ret_value = self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(ret_value));
                }
                Operator::Select => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error"); // must be i32
                    let else_val = ctrl_stack.current_stack().pop().expect("Stack error");
                    let then_val = ctrl_stack.current_stack().pop().expect("Stack error");

                    let bool_value = llvm_builder.build_int_compare(
                        IntPredicate::NE,
                        int_value,
                        int_value.get_type().const_zero(),
                        FUNCTION_TMP_VAR_NAME,
                    );

                    let value = match then_val {
                        StackValue::IntValue(then_val) => match else_val {
                            StackValue::IntValue(else_val) => StackValue::IntValue(
                                llvm_builder
                                    .build_select(
                                        bool_value,
                                        then_val,
                                        else_val,
                                        FUNCTION_TMP_VAR_NAME,
                                    )
                                    .into_int_value(),
                            ),
                            _ => panic!("Select arguments must have same type"),
                        },
                        StackValue::FloatValue(then_val) => match else_val {
                            StackValue::FloatValue(else_val) => StackValue::FloatValue(
                                llvm_builder
                                    .build_select(
                                        bool_value,
                                        then_val,
                                        else_val,
                                        FUNCTION_TMP_VAR_NAME,
                                    )
                                    .into_int_value(),
                            ),
                            _ => panic!("Select arguments must have same type"),
                        },
                    };

                    ctrl_stack.current_stack().push(value)
                }
                Operator::I64ExtendI32S
                | Operator::I64ExtendI32U
                | Operator::I32WrapI64
                | Operator::I32Extend8S
                | Operator::I32Extend16S
                | Operator::I64Extend8S
                | Operator::I64Extend16S
                | Operator::I64Extend32S => {
                    if let Ok(int_value) = ctrl_stack.current_stack().pop_int() {
                        let int64_type = llvm_context.i64_type();
                        let int32_type = llvm_context.i32_type();
                        let int16_type = llvm_context.i16_type();
                        let int8_type = llvm_context.i8_type();
                        let value = match op {
                            Operator::I64ExtendI32U => llvm_builder.build_int_z_extend(
                                int_value,
                                int64_type,
                                FUNCTION_TMP_VAR_NAME,
                            ),
                            Operator::I64ExtendI32S => llvm_builder.build_int_s_extend(
                                int_value,
                                int64_type,
                                FUNCTION_TMP_VAR_NAME,
                            ),
                            Operator::I32WrapI64 => llvm_builder.build_int_truncate(
                                int_value,
                                int32_type,
                                FUNCTION_TMP_VAR_NAME,
                            ),
                            Operator::I32Extend8S => {
                                let val = llvm_builder.build_int_truncate(
                                    int_value,
                                    int8_type,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                llvm_builder.build_int_s_extend(
                                    val,
                                    int32_type,
                                    FUNCTION_TMP_VAR_NAME,
                                )
                            }
                            Operator::I32Extend16S => {
                                let val = llvm_builder.build_int_truncate(
                                    int_value,
                                    int16_type,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                llvm_builder.build_int_s_extend(
                                    val,
                                    int32_type,
                                    FUNCTION_TMP_VAR_NAME,
                                )
                            }
                            Operator::I64Extend8S => {
                                let val = llvm_builder.build_int_truncate(
                                    int_value,
                                    int8_type,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                llvm_builder.build_int_s_extend(
                                    val,
                                    int64_type,
                                    FUNCTION_TMP_VAR_NAME,
                                )
                            }
                            Operator::I64Extend16S => {
                                let val = llvm_builder.build_int_truncate(
                                    int_value,
                                    int16_type,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                llvm_builder.build_int_s_extend(
                                    val,
                                    int64_type,
                                    FUNCTION_TMP_VAR_NAME,
                                )
                            }
                            Operator::I64Extend32S => {
                                let val = llvm_builder.build_int_truncate(
                                    int_value,
                                    int32_type,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                llvm_builder.build_int_s_extend(
                                    val,
                                    int64_type,
                                    FUNCTION_TMP_VAR_NAME,
                                )
                            }
                            _ => unreachable!(),
                        };
                        ctrl_stack.current_stack().push(StackValue::IntValue(value));
                    } else {
                        todo!()
                    }
                }
                Operator::I32TruncF32S
                | Operator::I32TruncF32U
                | Operator::I32TruncF64S
                | Operator::I32TruncF64U
                | Operator::I32TruncSatF32S
                | Operator::I32TruncSatF32U
                | Operator::I32TruncSatF64S
                | Operator::I32TruncSatF64U
                | Operator::I64TruncF32S
                | Operator::I64TruncF32U
                | Operator::I64TruncF64S
                | Operator::I64TruncF64U
                | Operator::I64TruncSatF32S
                | Operator::I64TruncSatF32U
                | Operator::I64TruncSatF64S
                | Operator::I64TruncSatF64U => {
                    todo!()
                }
                Operator::I32ReinterpretF32 | Operator::I64ReinterpretF64 => {
                    let float_as_int = ctrl_stack.current_stack().pop_float().expect("Stack error");
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(float_as_int));
                }
                Operator::F32ReinterpretI32 | Operator::F64ReinterpretI64 => {
                    let int_as_float = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(int_as_float));
                }
                Operator::F32DemoteF64
                | Operator::F32ConvertI32S
                | Operator::F32ConvertI32U
                | Operator::F32ConvertI64S
                | Operator::F32ConvertI64U
                | Operator::F64PromoteF32
                | Operator::F64ConvertI64S => {
                    unimplemented!("{:?}", op)
                }
                Operator::F64ConvertI32S | Operator::F64ConvertI32U => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let i64_type = llvm_context.i64_type();
                    let int64_value =
                        llvm_builder.build_int_z_extend(int_value, i64_type, FUNCTION_TMP_VAR_NAME);

                    let float_op = if matches!(op, Operator::F64ConvertI32S) {
                        i64_type.const_int(SyscallMathOp::F64ConvertI32S.into(), false)
                    } else {
                        i64_type.const_int(SyscallMathOp::F64ConvertI32U.into(), false)
                    };
                    let zero = i64_type.const_zero();
                    let args = [float_op.into(), int64_value.into(), zero.into()];
                    let float_ret = self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(float_ret));
                }
                Operator::F64ConvertI64U => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let float_op = llvm_context
                        .i64_type()
                        .const_int(SyscallMathOp::F64ConvertI64U.into(), false);
                    let zero = llvm_context.i64_type().const_zero();
                    let args = [float_op.into(), int_value.into(), zero.into()];
                    let float_ret = self.build_call_math_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::FloatValue(float_ret));
                }
                Operator::Block { blockty }
                | Operator::If { blockty }
                | Operator::Loop { blockty } => {
                    let generate_result_variable = |ty: ValType, name: &str| {
                        let value = local_var_builder.get(llvm_builder, name);
                        LocalVar { ty, value }
                    };
                    let (res_var_name, param_var_name) = match op {
                        Operator::Block { .. } => (BLOCK_RESULT_VAR_NAME, ""),
                        Operator::If { .. } => (IF_RESULT_VAR_NAME, ""),
                        Operator::Loop { .. } => (LOOP_RESULT_VAR_NAME, LOOP_PARAM_VAR_NAME),
                        _ => unreachable!(),
                    };

                    let (params, results) = match blockty {
                        cranelift_wasm::wasmparser::BlockType::Empty => (Vec::new(), Vec::new()),
                        cranelift_wasm::wasmparser::BlockType::Type(ty) => {
                            let result = generate_result_variable(ty, BLOCK_RESULT_VAR_NAME);
                            (Vec::new(), vec![result])
                        }
                        cranelift_wasm::wasmparser::BlockType::FuncType(type_index) => {
                            let fn_signature = self
                                .signatures
                                .get(type_index as usize)
                                .expect("Can't find block type");
                            let results = fn_signature
                                .returns
                                .iter()
                                .map(|ty| generate_result_variable(ty.clone().into(), res_var_name))
                                .collect();
                            let params = fn_signature
                                .params
                                .iter()
                                .map(|ty| {
                                    generate_result_variable(ty.clone().into(), param_var_name)
                                })
                                .collect();
                            (params, results)
                        }
                    };
                    match op {
                        Operator::Block { .. } => ctrl_stack.append_block(
                            llvm_context,
                            llvm_builder,
                            params.len(),
                            results,
                        ),
                        Operator::If { .. } => {
                            let int_value =
                                ctrl_stack.current_stack().pop_int().expect("Stack error");
                            // LLVM requires a boolean value in condition but LLVM doesn't have a way to cast i32 to i1 in the way we need.
                            // So let's emulate that.
                            let zero = int_value.get_type().const_zero();
                            let bool_value = llvm_builder.build_int_compare(
                                IntPredicate::NE,
                                zero,
                                int_value,
                                FUNCTION_TMP_VAR_NAME,
                            );

                            ctrl_stack.append_if_else(
                                &llvm_context,
                                &llvm_builder,
                                params.len(),
                                results,
                            );
                            let if_then = ctrl_stack.top(); // get the just added block to generate a conditional branch
                            if let Some(ControlBlock::IfElse { then, if_else, .. }) = if_then {
                                llvm_builder.build_conditional_branch(bool_value, then, if_else);
                                llvm_builder.position_at_end(then);
                            };
                        }
                        Operator::Loop { .. } => {
                            ctrl_stack.append_loop(&llvm_context, &llvm_builder, params, results);
                            let loop_block = ctrl_stack.top();
                            if let Some(ControlBlock::Loop {
                                loop_start, params, ..
                            }) = loop_block
                            {
                                // Store Loop parameters
                                for var in params.iter().rev() {
                                    let val = ctrl_stack.current_stack().pop().unwrap();
                                    match val {
                                        StackValue::IntValue(v) => {
                                            llvm_builder.build_store(var.value, v)
                                        }
                                        StackValue::FloatValue(v) => {
                                            llvm_builder.build_store(var.value, v)
                                        }
                                    };
                                }

                                llvm_builder.build_unconditional_branch(loop_start);
                                llvm_builder.position_at_end(loop_start);

                                // Load Loop parameters
                                for var in params.iter() {
                                    let pointee_ty =
                                        get_llvm_type(llvm_context, var.ty).as_basic_type_enum();
                                    let val = llvm_builder.build_load(
                                        pointee_ty,
                                        var.value,
                                        FUNCTION_TMP_VAR_NAME,
                                    );
                                    match var.ty {
                                        ValType::I32 | ValType::I64 => ctrl_stack
                                            .current_stack()
                                            .push(StackValue::IntValue(val.into_int_value())),
                                        ValType::F32 | ValType::F64 => ctrl_stack
                                            .current_stack()
                                            .push(StackValue::FloatValue(val.into_int_value())),
                                        _ => todo!(),
                                    };
                                }
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                Operator::Else => {
                    let if_then = ctrl_stack.top();
                    if let Some(ControlBlock::IfElse {
                        end,
                        if_else,
                        results,
                        value_stack,
                        ..
                    }) = if_then
                    {
                        ctrl_stack.set_else_is_found();

                        // Store results
                        self.store_to_local_variables(&results, &value_stack);

                        ctrl_stack.reinit_ifelse_value_stack();
                        llvm_builder.build_unconditional_branch(end);
                        llvm_builder.position_at_end(if_else);
                    } else {
                        panic!("Found Else without If");
                    }
                }
                Operator::Return | Operator::Br { .. } => {
                    let relative_depth = if let Operator::Br { relative_depth } = op {
                        relative_depth
                    } else {
                        ctrl_stack.max_depth()
                    };
                    let block = ctrl_stack.peek(relative_depth).expect("Stack error");

                    match &block {
                        ControlBlock::Loop { params, .. } => {
                            // Store params
                            for (idx, ptr) in params.iter().rev().enumerate() {
                                let value = ctrl_stack
                                    .current_stack()
                                    .peek(idx as u32)
                                    .expect("Stack error");
                                match value {
                                    StackValue::IntValue(val) => {
                                        llvm_builder.build_store(ptr.value, val)
                                    }
                                    StackValue::FloatValue(val) => {
                                        llvm_builder.build_store(ptr.value, val)
                                    }
                                };
                            }
                        }
                        _ => {
                            // Store results
                            let results = block.get_results();
                            self.store_to_local_variables(&results, ctrl_stack.current_stack());
                        }
                    }

                    // Generate jump
                    let destination_block = block.get_end_basic_block();
                    llvm_builder.build_unconditional_branch(destination_block);
                    // br instruction is a terminator and the LLVM verifier reuqires there shouldn't be a terminator in the middle of a basic block.
                    // So need to create a new basic block
                    let current_block = llvm_builder.get_insert_block().unwrap();
                    let dummy = llvm_context.insert_basic_block_after(current_block, "");
                    llvm_builder.position_at_end(dummy);

                    ctrl_stack.set_stack_polymorphic();
                }
                Operator::BrIf { relative_depth } => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let block = ctrl_stack.peek(relative_depth).expect("Stack error");

                    let destination_block = block.get_end_basic_block();
                    let current_block = llvm_builder
                        .get_insert_block()
                        .expect("Can't get current block");
                    let else_block = llvm_context.insert_basic_block_after(current_block, "Else");

                    self.store_results_and_params_before_jump(&block, ctrl_stack.current_stack());

                    // LLVM requires a boolean value in condition but LLVM doesn't have a way to cast i32 to i1 in the way we need.
                    // So let's emulate that.
                    let zero = int_value.get_type().const_zero();
                    let bool_value = llvm_builder.build_int_compare(
                        IntPredicate::NE,
                        zero,
                        int_value,
                        FUNCTION_TMP_VAR_NAME,
                    );
                    llvm_builder.build_conditional_branch(
                        bool_value,
                        destination_block,
                        else_block,
                    );
                    llvm_builder.position_at_end(else_block);
                }
                Operator::BrTable { targets } => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let default_case = targets.default();
                    let else_block = ctrl_stack
                        .peek(default_case)
                        .expect("Can't peek a target block");

                    self.store_results_and_params_before_jump(
                        &else_block,
                        ctrl_stack.current_stack(),
                    );

                    let mut cases = Vec::<(IntValue, BasicBlock)>::new();
                    for (idx, depth) in targets.targets().enumerate() {
                        let dep = depth.expect("Can't read br_table cases");
                        let case_block = ctrl_stack.peek(dep).expect("Can't peek a target block");
                        let case_value = int_value.get_type().const_int(idx as u64, false);

                        self.store_results_and_params_before_jump(
                            &case_block,
                            ctrl_stack.current_stack(),
                        );

                        cases.push((case_value, case_block.get_end_basic_block()));
                    }

                    llvm_builder.build_switch(int_value, else_block.get_end_basic_block(), &cases);
                    let current_block = llvm_builder.get_insert_block().unwrap();
                    // br instruction is a terminator and the LLVM verifier reuqires there shouldn't be a terminator in the middle of a basic block.
                    // So need to create a new basic block
                    let dummy = llvm_context.insert_basic_block_after(current_block, "");
                    llvm_builder.position_at_end(dummy);

                    ctrl_stack.set_stack_polymorphic();
                }
                Operator::End => {
                    let if_then = ctrl_stack.top();
                    if let Some(ControlBlock::IfElse {
                        end,
                        if_else,
                        results,
                        value_stack,
                        ..
                    }) = if_then
                    {
                        // This is a case when "else" instruction is missed
                        if !ctrl_stack.get_else_is_found() {
                            // Store results
                            self.store_to_local_variables(&results, &value_stack);

                            ctrl_stack.reinit_ifelse_value_stack();
                            llvm_builder.build_unconditional_branch(end);
                            llvm_builder.position_at_end(if_else);
                        }
                    }

                    let block = ctrl_stack.pop().expect("Label Stack error");
                    match block {
                        ControlBlock::IfElse {
                            end: block_end,
                            params_count,
                            results,
                            value_stack,
                            stack_polymorphic,
                            ..
                        }
                        | ControlBlock::Loop {
                            loop_end: block_end,
                            params_count,
                            results,
                            value_stack,
                            stack_polymorphic,
                            ..
                        }
                        | ControlBlock::Block {
                            block_end,
                            params_count,
                            results,
                            value_stack,
                            stack_polymorphic,
                        } => {
                            if !stack_polymorphic {
                                // Store results
                                self.store_to_local_variables(&results, &value_stack);
                            }

                            llvm_builder.build_unconditional_branch(block_end);
                            llvm_builder.position_at_end(block_end);

                            ctrl_stack.current_stack().drop(params_count);

                            // Load results
                            self.load_from_local_variables(&results, &mut ctrl_stack);
                        }
                        ControlBlock::Function {
                            end: block_end,
                            results,
                            value_stack,
                            stack_polymorphic,
                            ..
                        } => {
                            if !stack_polymorphic {
                                // Store results
                                self.store_to_local_variables(&results, &value_stack);
                            }

                            llvm_builder.build_unconditional_branch(block_end);
                            llvm_builder.position_at_end(block_end);

                            assert!(results.len() < 2, "TODO: Function can't return an array");
                            // Load results
                            if let Some(var) = results.iter().nth(0) {
                                let pointee_ty =
                                    get_llvm_type(llvm_context, var.ty).as_basic_type_enum();
                                let value = llvm_builder.build_load(
                                    pointee_ty,
                                    var.value,
                                    FUNCTION_TMP_VAR_NAME,
                                );
                                match var.ty {
                                    ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64 => {
                                        llvm_builder.build_return(Some(&value))
                                    }
                                    _ => todo!(),
                                };
                            } else {
                                llvm_builder.build_return(None);
                            }
                        }
                    }
                }
                Operator::Drop => {
                    ctrl_stack.current_stack().pop();
                }
                Operator::Nop => {}
                Operator::Unreachable => {
                    ctrl_stack.set_stack_polymorphic();

                    llvm_builder.build_unreachable();

                    let current_block = llvm_builder.get_insert_block().unwrap();
                    let dummy = llvm_context.insert_basic_block_after(current_block, "");
                    llvm_builder.position_at_end(dummy);
                }
                Operator::I32Load { memarg }
                | Operator::I32Load16S { memarg }
                | Operator::I32Load16U { memarg }
                | Operator::I32Load8S { memarg }
                | Operator::I32Load8U { memarg }
                | Operator::I64Load { memarg }
                | Operator::I64Load32S { memarg }
                | Operator::I64Load32U { memarg }
                | Operator::I64Load16S { memarg }
                | Operator::I64Load16U { memarg }
                | Operator::I64Load8S { memarg }
                | Operator::I64Load8U { memarg }
                | Operator::F64Load { memarg }
                | Operator::F32Load { memarg } => {
                    let mem_addr = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let offset = llvm_context.i32_type().const_int(memarg.offset, false);

                    let ret_value = match op {
                        Operator::I64Load { .. }
                        | Operator::F64Load { .. }
                        | Operator::I64Load32U { .. }
                        | Operator::I64Load16U { .. }
                        | Operator::I64Load8U { .. }
                        | Operator::I64Load32S { .. }
                        | Operator::I64Load16S { .. }
                        | Operator::I64Load8S { .. } => {
                            let args = [
                                llvm_context
                                    .i32_type()
                                    .const_int(SyscallMemoryOp::from(&op).into(), false)
                                    .into(),
                                mem_addr.into(),
                                offset.into(),
                                llvm_context.i64_type().const_zero().into(),
                            ];
                            self.build_call_memory_op_syscall64(&args, FUNCTION_TMP_VAR_NAME)
                        }
                        Operator::I32Load { .. }
                        | Operator::F32Load { .. }
                        | Operator::I32Load16U { .. }
                        | Operator::I32Load8U { .. }
                        | Operator::I32Load16S { .. }
                        | Operator::I32Load8S { .. } => {
                            let args = [
                                llvm_context
                                    .i32_type()
                                    .const_int(SyscallMemoryOp::from(&op).into(), false)
                                    .into(),
                                mem_addr.into(),
                                offset.into(),
                                llvm_context.i32_type().const_zero().into(),
                            ];
                            self.build_call_memory_op_syscall32(&args, FUNCTION_TMP_VAR_NAME)
                        }
                        _ => unreachable!(),
                    };

                    if matches!(op, Operator::F64Load { .. } | Operator::F32Load { .. }) {
                        ctrl_stack
                            .current_stack()
                            .push(StackValue::FloatValue(ret_value));
                    } else {
                        ctrl_stack
                            .current_stack()
                            .push(StackValue::IntValue(ret_value))
                    }
                }
                Operator::I32Store { memarg }
                | Operator::I32Store16 { memarg }
                | Operator::I32Store8 { memarg }
                | Operator::I64Store { memarg }
                | Operator::I64Store32 { memarg }
                | Operator::I64Store16 { memarg }
                | Operator::I64Store8 { memarg }
                | Operator::F32Store { memarg }
                | Operator::F64Store { memarg } => {
                    let int_value = ctrl_stack.current_stack().pop().expect("Stack error");
                    let mem_addr = ctrl_stack.current_stack().pop_int().expect("Stack error");
                    let int_value = match int_value {
                        StackValue::FloatValue(v) => v,
                        StackValue::IntValue(v) => v,
                    };

                    let offset = llvm_context.i32_type().const_int(memarg.offset, false);

                    let args = [
                        llvm_context
                            .i32_type()
                            .const_int(SyscallMemoryOp::from(&op).into(), false)
                            .into(),
                        mem_addr.into(),
                        offset.into(),
                        int_value.into(),
                    ];
                    match op {
                        Operator::I64Store { .. }
                        | Operator::F64Store { .. }
                        | Operator::I64Store32 { .. }
                        | Operator::I64Store16 { .. }
                        | Operator::I64Store8 { .. } => {
                            self.build_call_memory_op_syscall64(&args, FUNCTION_TMP_VAR_NAME);
                        }
                        Operator::I32Store { .. }
                        | Operator::F32Store { .. }
                        | Operator::I32Store16 { .. }
                        | Operator::I32Store8 { .. } => {
                            self.build_call_memory_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);
                        }
                        _ => unreachable!(),
                    };
                }
                Operator::MemorySize {
                    mem: _,
                    mem_byte: _,
                } => {
                    let args = [
                        llvm_context
                            .i32_type()
                            .const_int(SyscallMemoryOp::from(&op).into(), false)
                            .into(),
                        llvm_context.i32_type().const_zero().into(),
                        llvm_context.i32_type().const_zero().into(),
                        llvm_context.i32_type().const_zero().into(),
                    ];
                    let ret_value =
                        self.build_call_memory_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(ret_value));
                }
                Operator::MemoryGrow {
                    mem: _,
                    mem_byte: _,
                } => {
                    let int_value = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let args = [
                        llvm_context
                            .i32_type()
                            .const_int(SyscallMemoryOp::from(&op).into(), false)
                            .into(),
                        int_value.into(),
                        llvm_context.i32_type().const_zero().into(),
                        llvm_context.i32_type().const_zero().into(),
                    ];
                    let ret_value =
                        self.build_call_memory_op_syscall32(&args, FUNCTION_TMP_VAR_NAME);

                    ctrl_stack
                        .current_stack()
                        .push(StackValue::IntValue(ret_value));
                }
                Operator::MemoryFill { mem: _ } => {
                    unimplemented!("{:?}", op)
                }
                Operator::MemoryCopy {
                    dst_mem: _,
                    src_mem: _,
                } => {
                    unimplemented!("{:?}", op)
                }
                Operator::MemoryInit {
                    data_index: _,
                    mem: _,
                } => {
                    unimplemented!("{:?}", op)
                }
                Operator::DataDrop { data_index: _ } => {
                    unimplemented!("{:?}", op)
                }
                Operator::CallIndirect {
                    type_index,
                    table_index,
                    table_byte: _,
                } => {
                    let variant = ctrl_stack.current_stack().pop_int().expect("Stack error");

                    let target_fn_signature = &self.signatures[type_index as usize];
                    let table = self
                        .tables
                        .get(table_index as usize)
                        .expect("Can't find a table");
                    let possible_target_indexes = table
                        .original_data
                        .iter()
                        .enumerate()
                        .filter_map(|(idx, e)| {
                            let function = self
                                .functions
                                .get(e.as_u32() as usize)
                                .expect("Can't find a function");
                            if function.type_idx.as_u32() == type_index {
                                Some(idx as u64)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let current_block = llvm_builder.get_insert_block().unwrap();

                    let args = self.store_function_arguments(target_fn_signature, &mut ctrl_stack);
                    let result_var =
                        local_var_builder.get(llvm_builder, INDIRECT_CALL_RESULT_VAR_NAME);

                    let mut cases = Vec::<(IntValue, BasicBlock)>::new();
                    for idx in &possible_target_indexes {
                        let cur_block = llvm_builder.get_insert_block().unwrap();
                        let case = llvm_context
                            .insert_basic_block_after(cur_block, INDIRECT_CALL_CASE_NAME);
                        llvm_builder.position_at_end(case);
                        let value = llvm_context.i32_type().const_int(*idx, false);
                        cases.push((value, case));
                    }
                    let else_block = {
                        let cur_block = llvm_builder.get_insert_block().unwrap();
                        llvm_context.insert_basic_block_after(cur_block, INDIRECT_CALL_ELSE_NAME)
                    };
                    let switch_end_block =
                        llvm_context.insert_basic_block_after(else_block, INDIRECT_CALL_END_NAME);

                    for (idx, (_, block)) in possible_target_indexes.iter().zip(cases.iter()) {
                        llvm_builder.position_at_end(*block);

                        let fn_idx = table
                            .original_data
                            .get(*idx as usize)
                            .expect("Can't find a function");
                        let function = self
                            .functions
                            .get(fn_idx.as_u32() as usize)
                            .expect("Can't find a function");
                        let ret_value = llvm_builder.build_call(
                            function.function,
                            &args,
                            FUNCTION_TMP_VAR_NAME,
                        );

                        if let Some(ret_type) = target_fn_signature.returns.get(0) {
                            let value = match ret_type {
                                WasmType::I32 | WasmType::I64 | WasmType::F32 | WasmType::F64 => {
                                    ret_value
                                        .try_as_basic_value()
                                        .expect_left("Can't extract function's return value")
                                        .into_int_value()
                                }
                                _ => unimplemented!(),
                            };
                            llvm_builder.build_store(result_var, value);
                        }
                        llvm_builder.build_unconditional_branch(switch_end_block);
                    }

                    llvm_builder.position_at_end(else_block);
                    llvm_builder.build_unreachable();

                    llvm_builder.position_at_end(current_block);

                    // LLVM doesn't allow a constant value in swtich variant field.
                    // This is a workaround, it just creates a variable which contains the constant value.
                    let variant = if variant.is_const() {
                        let tmp_var = local_var_builder.get(llvm_builder, FUNCTION_TMP_VAR_NAME);
                        llvm_builder.build_store(tmp_var, variant);
                        llvm_builder
                            .build_load(variant.get_type(), tmp_var, FUNCTION_TMP_VAR_NAME)
                            .into_int_value()
                    } else {
                        variant
                    };
                    llvm_builder.build_switch(variant, else_block, &cases);

                    llvm_builder.position_at_end(switch_end_block);
                    if let Some(ret_type) = target_fn_signature.returns.get(0) {
                        let pointee_ty = get_llvm_type(llvm_context, ret_type.clone().into());
                        let res = llvm_builder.build_load(
                            pointee_ty.as_basic_type_enum(),
                            result_var,
                            FUNCTION_TMP_VAR_NAME,
                        );

                        let value = match ret_type {
                            WasmType::I32 | WasmType::I64 | WasmType::F32 | WasmType::F64 => {
                                StackValue::IntValue(res.into_int_value())
                            }
                            _ => unimplemented!(),
                        };
                        ctrl_stack.current_stack().push(value);
                    }
                }
                Operator::Call { function_index } => {
                    let signature_idx = &self.functions[function_index as usize].type_idx;
                    let target_fn_signature = &self.signatures[signature_idx.as_u32() as usize];

                    let args = self.store_function_arguments(target_fn_signature, &mut ctrl_stack);

                    let function = self
                        .functions
                        .get(function_index as usize)
                        .expect("Can't find a function");
                    let ret_value =
                        llvm_builder.build_call(function.function, &args, FUNCTION_TMP_VAR_NAME);

                    if let Some(ret_type) = target_fn_signature.returns.get(0) {
                        let value = match ret_type {
                            WasmType::I32 | WasmType::I64 | WasmType::F32 | WasmType::F64 => {
                                StackValue::IntValue(
                                    ret_value
                                        .try_as_basic_value()
                                        .expect_left("Can't extract function's return value")
                                        .into_int_value(),
                                )
                            }
                            _ => unimplemented!(),
                        };
                        ctrl_stack.current_stack().push(value)
                    }
                }
                _ => {
                    unimplemented!("Operator {:?} is unsupported", op)
                }
            }
            // println!("{:?}: {:?}", pos, op);
        }
        let pos = reader.original_position();
        validator.finish(pos)?;

        Ok(())
    }

    fn store_to_local_variables(&self, variables: &Vec<LocalVar>, value_stack: &ValueStack) {
        for (idx, ptr) in variables.iter().enumerate() {
            let value = value_stack.peek(idx as u32).expect("Stack error");
            match value {
                StackValue::IntValue(val) => self.llvm_builder.build_store(ptr.value, val),
                StackValue::FloatValue(val) => self.llvm_builder.build_store(ptr.value, val),
            };
        }
    }

    fn load_from_local_variables<'b>(
        &self,
        variables: &Vec<LocalVar<'b>>,
        ctrl_stack: &mut ControlStack<'b>,
    ) where
        'a: 'b,
    {
        for var in variables.iter().rev() {
            let pointee_ty = get_llvm_type(&self.llvm_context, var.ty).as_basic_type_enum();
            let value: inkwell::values::BasicValueEnum<'b> =
                self.llvm_builder
                    .build_load(pointee_ty, var.value, FUNCTION_TMP_VAR_NAME);
            match var.ty {
                ValType::I32 | ValType::I64 => ctrl_stack
                    .current_stack()
                    .push(StackValue::IntValue(value.into_int_value())),
                ValType::F32 | ValType::F64 => ctrl_stack
                    .current_stack()
                    .push(StackValue::FloatValue(value.into_int_value())),
                _ => todo!(),
            }
        }
    }

    fn store_results_and_params_before_jump(
        &self,
        destination_block: &ControlBlock,
        value_stack: &ValueStack,
    ) {
        match destination_block {
            ControlBlock::Loop { params, .. } => {
                // Store params
                for (idx, ptr) in params.iter().rev().enumerate() {
                    let value = value_stack.peek(idx as u32).expect("Stack error");
                    match value {
                        StackValue::IntValue(val) => self.llvm_builder.build_store(ptr.value, val),
                        StackValue::FloatValue(val) => {
                            self.llvm_builder.build_store(ptr.value, val)
                        }
                    };
                }
            }
            _ => {
                // Store results
                let results = destination_block.get_results();
                self.store_to_local_variables(&results, value_stack);
            }
        }
    }

    fn add_fn_param_buffer(&mut self) {
        if self.fn_param_buffer.is_some() {
            return;
        }
        let max_num = self
            .signatures
            .iter()
            .map(|s| s.params.len())
            .max()
            .unwrap();

        let array_size = max_num as u32;
        let array_ty = self.llvm_context.i64_type().array_type(array_size);

        let global_var = self
            .llvm_module
            .add_global(array_ty, None, FN_PARAM_VAR_NAME);

        // Init the memory with zeros
        let zeros = (0..max_num)
            .map(|_| self.llvm_context.i64_type().const_zero())
            .collect::<Vec<IntValue>>();
        let zeros_array = self.llvm_context.i64_type().const_array(&zeros);

        global_var.set_initializer(&zeros_array);
        global_var.set_linkage(Linkage::Internal);

        self.fn_param_buffer = Some(global_var);
    }

    fn store_function_arguments<'b>(
        &self,
        target_fn_signature: &FnSignature,
        ctrl_stack: &mut ControlStack<'b>,
    ) -> Vec<BasicMetadataValueEnum<'b>>
    where
        'a: 'b,
    {
        let llvm_context = self.llvm_context;
        let llvm_builder = &self.llvm_builder;

        if target_fn_signature.params.len() > 5 {
            let array_size = target_fn_signature.params.len() as u32;
            let args_array = self.fn_param_buffer.unwrap().as_pointer_value();
            for arg_num in (0..array_size).rev() {
                let arg = match ctrl_stack.current_stack().pop() {
                    Some(value) => match value {
                        StackValue::IntValue(val) => val,
                        StackValue::FloatValue(val) => val,
                    },
                    None => panic!("Stack error"),
                };
                let idx = llvm_context.i64_type().const_int(arg_num as u64, false);
                let ptr = unsafe {
                    llvm_builder.build_gep(
                        llvm_context.i64_type(),
                        args_array,
                        &[idx],
                        FUNCTION_TMP_VAR_NAME,
                    )
                };

                llvm_builder.build_store(ptr, arg);
            }
            vec![args_array.into()]
        } else {
            (0..target_fn_signature.params.len())
                .map(|_| match ctrl_stack.current_stack().pop() {
                    Some(value) => match value {
                        StackValue::IntValue(val) => val.into(),
                        StackValue::FloatValue(val) => val.into(),
                    },
                    None => panic!("Stack error"),
                })
                .collect::<Vec<BasicMetadataValueEnum>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        }
    }

    fn generate_function_prolog<'b>(
        &'b self,
        llvm_function: &FunctionValue,
        fn_signature: &FnSignature,
        local_var_builder: &'b LocalVarBuilder,
    ) -> Vec<LocalVar<'b>>
    where
        'a: 'b,
    {
        let llvm_context = self.llvm_context;
        let llvm_builder = &self.llvm_builder;

        let mut params = Vec::new();

        if fn_signature.params.len() > 5 {
            let args_ptr = llvm_function.get_nth_param(0).unwrap().into_pointer_value();
            for local_n in 0..fn_signature.params.len() as u32 {
                let ty = fn_signature.params[local_n as usize].into();
                let llvm_type = get_llvm_type(llvm_context, ty);
                let ptr = local_var_builder.get(llvm_builder, FUNCTION_PARAM_NAME);

                params.push(LocalVar { ty, value: ptr });

                let pointee_ty = llvm_context.i64_type();
                let idx = llvm_context.i64_type().const_int(local_n as u64, false);
                let arg_ptr = unsafe {
                    llvm_builder.build_gep(pointee_ty, args_ptr, &[idx], FUNCTION_TMP_VAR_NAME)
                };
                let param_value = llvm_builder.build_load(
                    llvm_type.as_basic_type_enum(),
                    arg_ptr,
                    FUNCTION_TMP_VAR_NAME,
                );

                llvm_builder.build_store(ptr, param_value);
            }
        } else {
            // Alloc the stack memory space for local variables and store function parameters to it
            for local_n in 0..fn_signature.params.len() as u32 {
                let ty = fn_signature.params[local_n as usize].into();
                let ptr = local_var_builder.get(llvm_builder, FUNCTION_PARAM_NAME);

                params.push(LocalVar { ty, value: ptr });

                let param_value = llvm_function.get_nth_param(local_n).unwrap();
                llvm_builder.build_store(ptr, param_value);
            }
        }

        params
    }

    fn add_function(&mut self, func_type: TypeIndex, name: Option<String>, imported: bool) {
        let fn_signature = &self.signatures[func_type.as_u32() as usize];
        let func_index = self.functions.len();

        let fn_name = name.unwrap_or(format!("func_{}", func_index));

        let params: Vec<BasicMetadataTypeEnum> = if fn_signature.params.len() > 5 {
            let array_type = self
                .llvm_context
                .i64_type()
                .ptr_type(AddressSpace::default());
            vec![array_type.into()]
        } else {
            fn_signature
                .params
                .iter()
                .map(|wasm_ty| {
                    BasicMetadataTypeEnum::from(
                        get_llvm_type(&self.llvm_context, wasm_ty.clone().into())
                            .as_basic_type_enum(),
                    )
                })
                .collect()
        };
        let fn_type = if fn_signature.returns.len() == 0 {
            self.llvm_context.void_type().fn_type(&params, false)
        } else if fn_signature.returns.len() == 1 {
            get_llvm_type(&self.llvm_context, fn_signature.returns[0].into())
                .as_basic_type_enum()
                .fn_type(&params, false)
        } else {
            todo!(
                "At this moment the function can't return an array: name={}, func_index={}",
                fn_name,
                func_index
            );
        };

        let linkage = if imported {
            None
        } else {
            Some(Linkage::Internal)
        };

        let llvm_function = self.llvm_module.add_function(&fn_name, fn_type, linkage);
        llvm_function.add_attribute(
            AttributeLoc::Function,
            self.llvm_context
                .create_enum_attribute(Attribute::get_named_enum_kind_id("nosync"), 0),
        );
        llvm_function.add_attribute(
            AttributeLoc::Function,
            self.llvm_context
                .create_enum_attribute(Attribute::get_named_enum_kind_id("nounwind"), 0),
        );
        llvm_function.add_attribute(
            AttributeLoc::Function,
            self.llvm_context
                .create_enum_attribute(Attribute::get_named_enum_kind_id("nofree"), 0),
        );
        llvm_function.add_attribute(
            AttributeLoc::Function,
            self.llvm_context
                .create_string_attribute("frame-pointer", "all"),
        );
        llvm_function.add_attribute(
            AttributeLoc::Function,
            self.llvm_context
                .create_string_attribute("no-trapping-math", "true"),
        );

        self.functions.push(Function {
            function: llvm_function,
            type_idx: func_type,
            exportable: false,
            imported,
        });
    }

    fn add_math_op_syscalls(&mut self) {
        if self.math_op_syscall64.is_none() {
            // syscall_math_op(op: u64, arg1: u64, arg2: u64) -> u64
            let fn_name = "syscall_math_op64";
            let i64type = self.llvm_context.i64_type();

            let params = [i64type.into(), i64type.into(), i64type.into()];
            let fn_type = i64type.fn_type(&params, false);

            let function = self.llvm_module.add_function(&fn_name, fn_type, None);

            self.math_op_syscall64 = Some(function);
        }
        if self.math_op_syscall32.is_none() {
            let fn_name = "syscall_math_op32";
            let i32type = self.llvm_context.i32_type();

            let params = [i32type.into(), i32type.into(), i32type.into()];
            let fn_type = i32type.fn_type(&params, false);

            let function = self.llvm_module.add_function(&fn_name, fn_type, None);

            self.math_op_syscall32 = Some(function);
        }
    }

    fn add_memory_op_syscalls(&mut self) {
        if self.memory_op_syscall64.is_none() {
            let fn_name = "syscall_memory_op64";
            let i32type = self.llvm_context.i32_type();
            let i64type = self.llvm_context.i64_type();

            let params = [
                i32type.into(),
                i32type.into(),
                i32type.into(),
                i64type.into(),
            ];
            let fn_type = i64type.fn_type(&params, false);

            let function = self.llvm_module.add_function(&fn_name, fn_type, None);

            self.memory_op_syscall64 = Some(function);
        }
        if self.memory_op_syscall32.is_none() {
            let fn_name = "syscall_memory_op32";
            let i32type = self.llvm_context.i32_type();

            let params = [
                i32type.into(),
                i32type.into(),
                i32type.into(),
                i32type.into(),
            ];
            let fn_type = i32type.fn_type(&params, false);

            let function = self.llvm_module.add_function(&fn_name, fn_type, None);

            self.memory_op_syscall32 = Some(function);
        }
    }

    fn build_call_math_op_syscall64<'b>(
        &self,
        args: &[BasicMetadataValueEnum<'b>],
        name: &str,
    ) -> IntValue<'b>
    where
        'a: 'b,
    {
        let function = self.math_op_syscall64.unwrap();
        let site_value = self.llvm_builder.build_call(function, args, name);
        let ct_value = site_value
            .try_as_basic_value()
            .expect_left("Can't get a function's return value");

        ct_value.into_int_value()
    }

    fn build_call_math_op_syscall32<'b>(
        &self,
        args: &[BasicMetadataValueEnum<'b>],
        name: &str,
    ) -> IntValue<'b>
    where
        'a: 'b,
    {
        let function = self.math_op_syscall32.unwrap();
        let site_value = self.llvm_builder.build_call(function, args, name);
        let ct_value = site_value
            .try_as_basic_value()
            .expect_left("Can't get a function's return value");

        ct_value.into_int_value()
    }

    fn build_call_memory_op_syscall64<'b>(
        &self,
        args: &[BasicMetadataValueEnum<'b>],
        name: &str,
    ) -> IntValue<'b>
    where
        'a: 'b,
    {
        let function = self.memory_op_syscall64.unwrap();
        let site_value = self.llvm_builder.build_call(function, args, name);
        let ct_value = site_value
            .try_as_basic_value()
            .expect_left("Can't get a function's return value");

        ct_value.into_int_value()
    }

    fn build_call_memory_op_syscall32<'b>(
        &self,
        args: &[BasicMetadataValueEnum<'b>],
        name: &str,
    ) -> IntValue<'b>
    where
        'a: 'b,
    {
        let function = self.memory_op_syscall32.unwrap();
        let site_value = self.llvm_builder.build_call(function, args, name);
        let ct_value = site_value
            .try_as_basic_value()
            .expect_left("Can't get a function's return value");

        ct_value.into_int_value()
    }
}

fn get_llvm_type<'a>(llvm_context: &'a Context, ty: ValType) -> Box<dyn BasicType + 'a> {
    match ty {
        ValType::I32 => Box::new(llvm_context.i32_type().as_basic_type_enum()),
        ValType::I64 => Box::new(llvm_context.i64_type().as_basic_type_enum()),
        ValType::V128 => Box::new(llvm_context.i128_type().as_basic_type_enum()),
        ValType::F32 => Box::new(llvm_context.i32_type().as_basic_type_enum()),
        ValType::F64 => Box::new(llvm_context.i64_type().as_basic_type_enum()),
        _ => todo!("ty: {:?}", ty),
    }
}
