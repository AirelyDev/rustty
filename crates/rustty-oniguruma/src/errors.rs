//! rustty-oniguruma errors
//!
//! PORTS: ghostty pkg/oniguruma/errors.zig
//!
//! PORT STATUS: done. Faithful mirror of the reference's `Error` set,
//! `convertError`/`errorString`, `ErrorInfo` extern struct, and the
//! `error_code_map` code<->error table.

// const c = @import("c.zig").c;
// const Encoding = @import("types.zig").Encoding;
use crate::types::Encoding;
use oniguruma_sys as c;
use std::os::raw::c_int;

/// Maximum error message length.
// PORT NOTE: `c.ONIG_MAX_ERROR_MESSAGE_LEN` is a bindgen `u32` const.
pub const MAX_ERROR_LEN: u32 = c::ONIG_MAX_ERROR_MESSAGE_LEN;

/// Convert an Oniguruma error to an error.
// PORT NOTE: the reference `convertError` returns `!c_int` — `Ok(code)` for a
// non-negative code, else the mapped `Error` (falling back to `Error::Unknown`).
// The Zig `inline for (error_code_map)` linear scan is mirrored below over
// `ERROR_CODE_MAP` (a faithful map of the reference's comptime table).
pub fn convert_error(code: c_int) -> Result<c_int, Error> {
    if code >= 0 {
        return Ok(code);
    }
    for &(err, mapped) in ERROR_CODE_MAP {
        if mapped == code {
            return Err(err);
        }
    }

    Err(Error::Unknown)
}

/// Convert an error code to a string. buf must be at least
/// MAX_ERROR_LEN bytes long.
// PORT NOTE: `c.onig_error_code_to_str(buf.ptr, code)` is a C variadic; the
// reference passes only the two fixed args, so we do the same. `OnigUChar` is
// `c_uchar` (== `u8`), so `buf.as_mut_ptr()` matches `*mut OnigUChar` directly.
pub fn error_string(buf: &mut [u8], code: c_int) -> &mut [u8] {
    let len = unsafe { c::onig_error_code_to_str(buf.as_mut_ptr(), code) };
    &mut buf[..len as usize]
}

/// The Oniguruma error info type, matching the C type exactly.
// PORT NOTE: `extern struct { encoding: *Encoding, par: [*]u8, par_end: [*]u8 }`.
#[repr(C)]
pub struct ErrorInfo {
    pub encoding: *mut Encoding,
    pub par: *mut u8,
    pub par_end: *mut u8,
}

/// All possible Oniguruma errors.
// PORT NOTE: the reference `Error` is a Zig error set; the faithful Rust mirror
// is a fieldless enum with the SAME variant names (the frozen `oni::Error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Mismatch,
    NoSupportConfig,
    Abort,
    Memory,
    TypeBug,
    ParserBug,
    StackBug,
    UndefinedBytecode,
    UnexpectedBytecode,
    MatchStackLimitOver,
    ParseDepthLimitOver,
    RetryLimitInMatchOver,
    RetryLimitInSearchOver,
    SubexpCallLimitInSearchOver,
    DefaultEncodingIsNotSet,
    SpecifiedEncodingCantConvertToWideChar,
    FailToInitialize,
    InvalidArgument,
    EndPatternAtLeftBrace,
    EndPatternAtLeftBracket,
    EmptyCharClass,
    PrematureEndOfCharClass,
    EndPatternAtEscape,
    EndPatternAtMeta,
    EndPatternAtControl,
    MetaCodeSyntax,
    ControlCodeSyntax,
    CharClassValueAtEndOfRange,
    CharClassValueAtStartOfRange,
    UnmatchedRangeSpecifierInCharClass,
    TargetOfRepeatOperatorNotSpecified,
    TargetOfRepeatOperatorInvalid,
    NestedRepeatOperator,
    UnmatchedCloseParenthesis,
    EndPatternWithUnmatchedParenthesis,
    EndPatternInGroup,
    UndefinedGroupOption,
    InvalidGroupOption,
    InvalidPosixBracketType,
    InvalidLookBehindPattern,
    InvalidRepeatRangePattern,
    TooBigNumber,
    TooBigNumberForRepeatRange,
    UpperSmallerThanLowerInRepeatRange,
    EmptyRangeInCharClass,
    MismatchCodeLengthInClassRange,
    TooManyMultiByteRanges,
    TooShortMultiByteString,
    TooBigBackrefNumber,
    InvalidBackref,
    NumberedBackrefOrCallNotAllowed,
    TooManyCaptures,
    TooLongWideCharValue,
    UndefinedOperator,
    EmptyGroupName,
    InvalidGroupName,
    InvalidCharInGroupName,
    UndefinedNameReference,
    UndefinedGroupReference,
    MultiplexDefinedName,
    MultiplexDefinitionNameCall,
    NeverEndingRecursion,
    GroupNumberOverForCaptureHistory,
    InvalidCharPropertyName,
    InvalidIfElseSyntax,
    InvalidAbsentGroupPattern,
    InvalidAbsentGroupGeneratorPattern,
    InvalidCalloutPattern,
    InvalidCalloutName,
    UndefinedCalloutName,
    InvalidCalloutBody,
    InvalidCalloutTagName,
    InvalidCalloutArg,
    InvalidCodePointValue,
    InvalidWideCharValue,
    TooBigWideCharValue,
    NotSupportedEncodingCombination,
    InvalidCombinationOfOptions,
    TooManyUserDefinedObjects,
    TooLongPropertyName,
    VeryInefficientPattern,
    LibraryIsNotInitialized,
    Unknown,
}

// PORT NOTE: faithful map of the reference `error_code_map`
// (`[]const struct { Error, c_int }`): each Oniguruma return/error code paired
// with its wrapper `Error`. `Error::Unknown` has no code (it is the fallback).
const ERROR_CODE_MAP: &[(Error, c_int)] = &[
    (Error::Mismatch, c::ONIG_MISMATCH),
    (Error::NoSupportConfig, c::ONIG_NO_SUPPORT_CONFIG),
    (Error::Abort, c::ONIG_ABORT),
    (Error::Memory, c::ONIGERR_MEMORY),
    (Error::TypeBug, c::ONIGERR_TYPE_BUG),
    (Error::ParserBug, c::ONIGERR_PARSER_BUG),
    (Error::StackBug, c::ONIGERR_STACK_BUG),
    (Error::UndefinedBytecode, c::ONIGERR_UNDEFINED_BYTECODE),
    (Error::UnexpectedBytecode, c::ONIGERR_UNEXPECTED_BYTECODE),
    (
        Error::MatchStackLimitOver,
        c::ONIGERR_MATCH_STACK_LIMIT_OVER,
    ),
    (
        Error::ParseDepthLimitOver,
        c::ONIGERR_PARSE_DEPTH_LIMIT_OVER,
    ),
    (
        Error::RetryLimitInMatchOver,
        c::ONIGERR_RETRY_LIMIT_IN_MATCH_OVER,
    ),
    (
        Error::RetryLimitInSearchOver,
        c::ONIGERR_RETRY_LIMIT_IN_SEARCH_OVER,
    ),
    (
        Error::SubexpCallLimitInSearchOver,
        c::ONIGERR_SUBEXP_CALL_LIMIT_IN_SEARCH_OVER,
    ),
    (
        Error::DefaultEncodingIsNotSet,
        c::ONIGERR_DEFAULT_ENCODING_IS_NOT_SET,
    ),
    (
        Error::SpecifiedEncodingCantConvertToWideChar,
        c::ONIGERR_SPECIFIED_ENCODING_CANT_CONVERT_TO_WIDE_CHAR,
    ),
    (Error::FailToInitialize, c::ONIGERR_FAIL_TO_INITIALIZE),
    (Error::InvalidArgument, c::ONIGERR_INVALID_ARGUMENT),
    (
        Error::EndPatternAtLeftBrace,
        c::ONIGERR_END_PATTERN_AT_LEFT_BRACE,
    ),
    (
        Error::EndPatternAtLeftBracket,
        c::ONIGERR_END_PATTERN_AT_LEFT_BRACKET,
    ),
    (Error::EmptyCharClass, c::ONIGERR_EMPTY_CHAR_CLASS),
    (
        Error::PrematureEndOfCharClass,
        c::ONIGERR_PREMATURE_END_OF_CHAR_CLASS,
    ),
    (Error::EndPatternAtEscape, c::ONIGERR_END_PATTERN_AT_ESCAPE),
    (Error::EndPatternAtMeta, c::ONIGERR_END_PATTERN_AT_META),
    (
        Error::EndPatternAtControl,
        c::ONIGERR_END_PATTERN_AT_CONTROL,
    ),
    (Error::MetaCodeSyntax, c::ONIGERR_META_CODE_SYNTAX),
    (Error::ControlCodeSyntax, c::ONIGERR_CONTROL_CODE_SYNTAX),
    (
        Error::CharClassValueAtEndOfRange,
        c::ONIGERR_CHAR_CLASS_VALUE_AT_END_OF_RANGE,
    ),
    (
        Error::CharClassValueAtStartOfRange,
        c::ONIGERR_CHAR_CLASS_VALUE_AT_START_OF_RANGE,
    ),
    (
        Error::UnmatchedRangeSpecifierInCharClass,
        c::ONIGERR_UNMATCHED_RANGE_SPECIFIER_IN_CHAR_CLASS,
    ),
    (
        Error::TargetOfRepeatOperatorNotSpecified,
        c::ONIGERR_TARGET_OF_REPEAT_OPERATOR_NOT_SPECIFIED,
    ),
    (
        Error::TargetOfRepeatOperatorInvalid,
        c::ONIGERR_TARGET_OF_REPEAT_OPERATOR_INVALID,
    ),
    (
        Error::NestedRepeatOperator,
        c::ONIGERR_NESTED_REPEAT_OPERATOR,
    ),
    (
        Error::UnmatchedCloseParenthesis,
        c::ONIGERR_UNMATCHED_CLOSE_PARENTHESIS,
    ),
    (
        Error::EndPatternWithUnmatchedParenthesis,
        c::ONIGERR_END_PATTERN_WITH_UNMATCHED_PARENTHESIS,
    ),
    (Error::EndPatternInGroup, c::ONIGERR_END_PATTERN_IN_GROUP),
    (
        Error::UndefinedGroupOption,
        c::ONIGERR_UNDEFINED_GROUP_OPTION,
    ),
    (Error::InvalidGroupOption, c::ONIGERR_INVALID_GROUP_OPTION),
    (
        Error::InvalidPosixBracketType,
        c::ONIGERR_INVALID_POSIX_BRACKET_TYPE,
    ),
    (
        Error::InvalidLookBehindPattern,
        c::ONIGERR_INVALID_LOOK_BEHIND_PATTERN,
    ),
    (
        Error::InvalidRepeatRangePattern,
        c::ONIGERR_INVALID_REPEAT_RANGE_PATTERN,
    ),
    (Error::TooBigNumber, c::ONIGERR_TOO_BIG_NUMBER),
    (
        Error::TooBigNumberForRepeatRange,
        c::ONIGERR_TOO_BIG_NUMBER_FOR_REPEAT_RANGE,
    ),
    (
        Error::UpperSmallerThanLowerInRepeatRange,
        c::ONIGERR_UPPER_SMALLER_THAN_LOWER_IN_REPEAT_RANGE,
    ),
    (
        Error::EmptyRangeInCharClass,
        c::ONIGERR_EMPTY_RANGE_IN_CHAR_CLASS,
    ),
    (
        Error::MismatchCodeLengthInClassRange,
        c::ONIGERR_MISMATCH_CODE_LENGTH_IN_CLASS_RANGE,
    ),
    (
        Error::TooManyMultiByteRanges,
        c::ONIGERR_TOO_MANY_MULTI_BYTE_RANGES,
    ),
    (
        Error::TooShortMultiByteString,
        c::ONIGERR_TOO_SHORT_MULTI_BYTE_STRING,
    ),
    (
        Error::TooBigBackrefNumber,
        c::ONIGERR_TOO_BIG_BACKREF_NUMBER,
    ),
    (Error::InvalidBackref, c::ONIGERR_INVALID_BACKREF),
    (
        Error::NumberedBackrefOrCallNotAllowed,
        c::ONIGERR_NUMBERED_BACKREF_OR_CALL_NOT_ALLOWED,
    ),
    (Error::TooManyCaptures, c::ONIGERR_TOO_MANY_CAPTURES),
    (
        Error::TooLongWideCharValue,
        c::ONIGERR_TOO_LONG_WIDE_CHAR_VALUE,
    ),
    (Error::UndefinedOperator, c::ONIGERR_UNDEFINED_OPERATOR),
    (Error::EmptyGroupName, c::ONIGERR_EMPTY_GROUP_NAME),
    (Error::InvalidGroupName, c::ONIGERR_INVALID_GROUP_NAME),
    (
        Error::InvalidCharInGroupName,
        c::ONIGERR_INVALID_CHAR_IN_GROUP_NAME,
    ),
    (
        Error::UndefinedNameReference,
        c::ONIGERR_UNDEFINED_NAME_REFERENCE,
    ),
    (
        Error::UndefinedGroupReference,
        c::ONIGERR_UNDEFINED_GROUP_REFERENCE,
    ),
    (
        Error::MultiplexDefinedName,
        c::ONIGERR_MULTIPLEX_DEFINED_NAME,
    ),
    (
        Error::MultiplexDefinitionNameCall,
        c::ONIGERR_MULTIPLEX_DEFINITION_NAME_CALL,
    ),
    (
        Error::NeverEndingRecursion,
        c::ONIGERR_NEVER_ENDING_RECURSION,
    ),
    (
        Error::GroupNumberOverForCaptureHistory,
        c::ONIGERR_GROUP_NUMBER_OVER_FOR_CAPTURE_HISTORY,
    ),
    (
        Error::InvalidCharPropertyName,
        c::ONIGERR_INVALID_CHAR_PROPERTY_NAME,
    ),
    (
        Error::InvalidIfElseSyntax,
        c::ONIGERR_INVALID_IF_ELSE_SYNTAX,
    ),
    (
        Error::InvalidAbsentGroupPattern,
        c::ONIGERR_INVALID_ABSENT_GROUP_PATTERN,
    ),
    (
        Error::InvalidAbsentGroupGeneratorPattern,
        c::ONIGERR_INVALID_ABSENT_GROUP_GENERATOR_PATTERN,
    ),
    (
        Error::InvalidCalloutPattern,
        c::ONIGERR_INVALID_CALLOUT_PATTERN,
    ),
    (Error::InvalidCalloutName, c::ONIGERR_INVALID_CALLOUT_NAME),
    (
        Error::UndefinedCalloutName,
        c::ONIGERR_UNDEFINED_CALLOUT_NAME,
    ),
    (Error::InvalidCalloutBody, c::ONIGERR_INVALID_CALLOUT_BODY),
    (
        Error::InvalidCalloutTagName,
        c::ONIGERR_INVALID_CALLOUT_TAG_NAME,
    ),
    (Error::InvalidCalloutArg, c::ONIGERR_INVALID_CALLOUT_ARG),
    (
        Error::InvalidCodePointValue,
        c::ONIGERR_INVALID_CODE_POINT_VALUE,
    ),
    (
        Error::InvalidWideCharValue,
        c::ONIGERR_INVALID_WIDE_CHAR_VALUE,
    ),
    (
        Error::TooBigWideCharValue,
        c::ONIGERR_TOO_BIG_WIDE_CHAR_VALUE,
    ),
    (
        Error::NotSupportedEncodingCombination,
        c::ONIGERR_NOT_SUPPORTED_ENCODING_COMBINATION,
    ),
    (
        Error::InvalidCombinationOfOptions,
        c::ONIGERR_INVALID_COMBINATION_OF_OPTIONS,
    ),
    (
        Error::TooManyUserDefinedObjects,
        c::ONIGERR_TOO_MANY_USER_DEFINED_OBJECTS,
    ),
    (
        Error::TooLongPropertyName,
        c::ONIGERR_TOO_LONG_PROPERTY_NAME,
    ),
    (
        Error::VeryInefficientPattern,
        c::ONIGERR_VERY_INEFFICIENT_PATTERN,
    ),
    (
        Error::LibraryIsNotInitialized,
        c::ONIGERR_LIBRARY_IS_NOT_INITIALIZED,
    ),
];
