options gen2

var c2da_rt_heap : array<uint8>

var c2da_rt_next : uint64 = 0x0

var c2da_rt_alloc_addrs : array<uint64>

var c2da_rt_alloc_sizes : array<uint64>

var c2da_rt_alloc_live : array<bool>

def c2da_rt_init_heap() {
    if (length(c2da_rt_heap) == 0) {
        reserve(c2da_rt_heap, int(0x4000000))
        resize(c2da_rt_heap, 1)
    }
}
def c2da_rt_reset() {
    c2da_rt_next = 0x0
    resize(c2da_rt_alloc_addrs, 0)
    resize(c2da_rt_alloc_sizes, 0)
    resize(c2da_rt_alloc_live, 0)
}
def c2da_rt_malloc(size : uint64) : uint64 {
    c2da_rt_init_heap()
    if (size == 0x0) {
        return 0x0
    }
    var start : uint64 = c2da_rt_next
    var end : uint64 = start + size
    if (end > 0x4000000) {
        return 0x0
    }
    if (int(end) > length(c2da_rt_heap)) {
        resize(c2da_rt_heap, int(end))
    }
    c2da_rt_next = end
    var address : uint64 = unsafe(intptr(unsafe(addr(c2da_rt_heap[int(start)]))))
    var record : uint64 = uint64(length(c2da_rt_alloc_addrs))
    resize(c2da_rt_alloc_addrs, int(record + 0x1))
    resize(c2da_rt_alloc_sizes, int(record + 0x1))
    resize(c2da_rt_alloc_live, int(record + 0x1))
    c2da_rt_alloc_addrs[int(record)] = address
    c2da_rt_alloc_sizes[int(record)] = size
    c2da_rt_alloc_live[int(record)] = true
    return address
}
def c2da_rt_free(address : uint64) {
    var i : uint64 = 0x0
    while (int(i) < length(c2da_rt_alloc_addrs)) {
        if (c2da_rt_alloc_live[int(i)] && c2da_rt_alloc_addrs[int(i)] == address) {
            c2da_rt_alloc_live[int(i)] = false
            return
        }
        i = i + 0x1
    }
}
def c2da_rt_realloc(address : uint64; size : uint64) : uint64 {
    if (address == 0x0) {
        return c2da_rt_malloc(size)
    }
    if (size == 0x0) {
        c2da_rt_free(address)
        return 0x0
    }
    var i : uint64 = 0x0
    var old_size : uint64 = 0x0
    var found : bool = false
    while (int(i) < length(c2da_rt_alloc_addrs)) {
        if (c2da_rt_alloc_live[int(i)] && c2da_rt_alloc_addrs[int(i)] == address) {
            old_size = c2da_rt_alloc_sizes[int(i)]
            found = true
        }
        i = i + 0x1
    }
    if (found == false) {
        return 0x0
    }
    var replacement : uint64 = c2da_rt_malloc(size)
    if (replacement == 0x0) {
        return 0x0
    }
    var copy_size : uint64 = 0x0
    if (old_size < size) {
        copy_size = old_size
    } else {
        copy_size = size
    }
    c2da_rt_memcpy(replacement, address, copy_size)
    c2da_rt_free(address)
    return replacement
}
def c2da_rt_memset(dst : uint64; value : uint8; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = value
        i = i + 0x1
    }
    return dst
}
def c2da_rt_calloc(count : uint64; size : uint64) : uint64 {
    var total : uint64 = count * size
    if (size != 0x0 && total / size != count) {
        return 0x0
    }
    var address : uint64 = c2da_rt_malloc(total)
    if (address != 0x0) {
        c2da_rt_memset(address, uint8(0x0), total)
    }
    return address
}
def c2da_rt_memcpy(dst : uint64; src : uint64; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
        i = i + 0x1
    }
    return dst
}
def c2da_rt_memcmp(left : uint64; right : uint64; count : uint64) : int {
    var i : uint64 = 0x0
    while (i < count) {
        if (uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(left)))[int(i)])) < uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(right)))[int(i)]))) {
            return -1
        }
        if (uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(left)))[int(i)])) > uint(unsafe(unsafe(unsafe(reinterpret<uint8?>(right)))[int(i)]))) {
            return 1
        }
        i = i + 0x1
    }
    return 0
}
def c2da_rt_memmove(dst : uint64; src : uint64; count : uint64) : uint64 {
    if (dst < src) {
        var i : uint64 = 0x0
        while (i < count) {
            unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
            i = i + 0x1
        }
    } else {
        var i : uint64 = count
        while (i > 0x0) {
            i = i - 0x1
            unsafe(unsafe(unsafe(reinterpret<uint8?>(dst)))[int(i)]) = unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)])
        }
    }
    return dst
}
def c2da_rt_memchr(src : uint64; value : uint8; count : uint64) : uint64 {
    var i : uint64 = 0x0
    while (i < count) {
        if (unsafe(unsafe(unsafe(reinterpret<uint8?>(src)))[int(i)]) == value) {
            return src + i
        }
        i = i + 0x1
    }
    return 0x0
}
struct C2daVaArg {
    tag : int
    i64 : int64
    f64 : double
    raw : uint64
}
struct C2daVaCursor {
    index : int
}
def c2da_min_int(a : int; b : int) : int {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_int(a : int; b : int) : int {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_uint(a : uint; b : uint) : uint {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_uint(a : uint; b : uint) : uint {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_int64(a : int64; b : int64) : int64 {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_int64(a : int64; b : int64) : int64 {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_min_uint64(a : uint64; b : uint64) : uint64 {
    if (a < b) {
        return a
    } else {
        return b
    }
}
def c2da_max_uint64(a : uint64; b : uint64) : uint64 {
    if (a > b) {
        return a
    } else {
        return b
    }
}
def c2da_clip_uint(v : int) : uint {
    if (v < 0) {
        return 0x0
    }
    if (v > 255) {
        return 0xff
    }
    return uint(v)
}
def c2da_bool_to_uint(v : bool) : uint {
    if (v == true) {
        return 0x1
    } else {
        return 0x0
    }
}
def c2da___assert_fail(expr : void?; file : void?; line : uint; func : void?) {
}
typedef int64_t = int64
typedef uint32_t = uint
typedef uint64_t = uint64
typedef size_t = uint64
def c2da___bswap_16(var c2da___bsx : uint16) : uint16 {
    return uint16(int(c2da___bsx) >> int(8) & int(255) | (int(c2da___bsx) & int(255)) << int(8))
}
def c2da___bswap_32(var c2da___bsx_0 : uint) : uint {
    return (c2da___bsx_0 & uint(0xff000000)) >> uint(int(24)) | (c2da___bsx_0 & uint(0xff0000)) >> uint(int(8)) | (c2da___bsx_0 & uint(0xff00)) << uint(int(8)) | (c2da___bsx_0 & uint(0xff)) << uint(int(24))
}
def c2da___bswap_64(var c2da___bsx_1 : uint64) : uint64 {
    return (c2da___bsx_1 & uint64(0xff00000000000000uL)) >> uint64(int(56)) | (c2da___bsx_1 & uint64(0xff000000000000uL)) >> uint64(int(40)) | (c2da___bsx_1 & uint64(0xff0000000000uL)) >> uint64(int(24)) | (c2da___bsx_1 & uint64(0xff00000000uL)) >> uint64(int(8)) | (c2da___bsx_1 & uint64(0xff000000)) << uint64(int(8)) | (c2da___bsx_1 & uint64(0xff0000)) << uint64(int(24)) | (c2da___bsx_1 & uint64(0xff00)) << uint64(int(40)) | (c2da___bsx_1 & uint64(0xff)) << uint64(int(56))
}
def c2da___uint16_identity(var c2da___x : uint16) : uint16 {
    return c2da___x
}
def c2da___uint32_identity(var c2da___x_0 : uint) : uint {
    return c2da___x_0
}
def c2da___uint64_identity(var c2da___x_1 : uint64) : uint64 {
    return c2da___x_1
}
[export]
def short_wrap() : int {
    var s : int16 = int16(0)
    s = int16(int(32767))
    var c2da_postinc : int16 = s
    s = int16(int(s) + 1)
    c2da_postinc
    var c2da_fresh0 : int = int(0)
    if (int(s) == -int(32768)) {
        c2da_fresh0 = int(1)
    }
    return c2da_fresh0
}
[export]
def ushort_wrap() : int {
    var u : uint16 = uint16(0)
    u = uint16(int(65535))
    var c2da_postinc_0 : uint16 = u
    u = uint16(int(u) + 1)
    c2da_postinc_0
    var c2da_fresh1 : int = int(0)
    if (int(u) == int(0)) {
        c2da_fresh1 = int(1)
    }
    return c2da_fresh1
}
[export]
def short_assign() : int {
    var s_0 : int16 = int16(0)
    s_0 = int16(int16(70000))
    var c2da_fresh2 : int = int(0)
    if (int(s_0) == int(4464)) {
        c2da_fresh2 = int(1)
    }
    return c2da_fresh2
}
[export]
def uchar_promotion() : int {
    var uc : uint8 = uint8(0)
    var i : int = int(0)
    uc = uint8(int(10))
    i = -int(20)
    var c2da_fresh3 : int = int(0)
    if (int(uc) + i < int(0)) {
        c2da_fresh3 = int(1)
    }
    return c2da_fresh3
}
[export]
def narrowing() : int {
    var a : int = int(0)
    a = int(300)
    return int(int8(a)) + int(uint8(a))
}
[export]
def negative_index() : int {
    var p : int? = null
    var q : int? = null
    var i_0 : int64 = int64(0)
    var r : int = int(0)
    p = unsafe(unsafe(reinterpret<int?>(unsafe(unsafe(reinterpret<int?>(c2da_rt_malloc(uint64(uint64(int(4)) * uint64(4)))))))))
    unsafe(unsafe(unsafe(reinterpret<int?>(p)))[int(0)]) = int(1)
    unsafe(unsafe(unsafe(reinterpret<int?>(p)))[int(1)]) = int(2)
    unsafe(unsafe(unsafe(reinterpret<int?>(p)))[int(2)]) = int(3)
    unsafe(unsafe(unsafe(reinterpret<int?>(p)))[int(3)]) = int(4)
    q = unsafe(addr(unsafe(unsafe(unsafe(reinterpret<int?>(p)))[int(2)])))
    i_0 = int64(-int(1))
    r = unsafe(unsafe(unsafe(reinterpret<int?>(q)))[int(i_0)]) * int(10) + unsafe(unsafe(unsafe(reinterpret<int?>(q)))[-int(2)])
    c2da_rt_free(unsafe(unsafe(reinterpret<uint64>(unsafe(unsafe(reinterpret<uint8?>(p)))))))
    return r
}
[export]
def compound() : int {
    var i_1 : int = int(0)
    var u_0 : uint = uint(0)
    var l : int64 = int64(0)
    var c : int8 = int8(0)
    var uc_0 : uint8 = uint8(0)
    i_1 = int(10)
    i_1 = int(i_1 * int(3))
    u_0 = uint(int(7))
    i_1 = int(uint(i_1) + u_0)
    l = int64(int(5))
    i_1 = int(int64(i_1) - l)
    c = int8(int(100))
    c = int8(int(c) + int(100))
    uc_0 = uint8(int(250))
    uc_0 = uint8(int(uc_0) + int(10))
    c = int8(int(c) & ~int(1))
    return i_1 * int(1000) + int(c) * int(10) + int(uc_0)
}
[export]
def incdec() : int {
    var c_0 : uint8 = uint8(0)
    var d : int8 = int8(0)
    var p_0 : int? = null
    var q_0 : int? = null
    var v : int = int(0)
    var w : int = int(0)
    var x : int = int(0)
    c_0 = uint8(int(255))
    var c2da_postinc_1 : uint8 = c_0
    c_0 = uint8(int(c_0) + 1)
    c2da_postinc_1
    d = int8(int(127))
    d = int8(int(d) + 1)
    d
    p_0 = unsafe(unsafe(reinterpret<int?>(unsafe(unsafe(reinterpret<int?>(c2da_rt_malloc(uint64(uint64(int(3)) * uint64(4)))))))))
    unsafe(unsafe(unsafe(reinterpret<int?>(p_0)))[int(0)]) = int(5)
    unsafe(unsafe(unsafe(reinterpret<int?>(p_0)))[int(1)]) = int(6)
    unsafe(unsafe(unsafe(reinterpret<int?>(p_0)))[int(2)]) = int(7)
    q_0 = p_0
    var c2da_postinc_2 : int? = q_0
    q_0 = unsafe(q_0 + 1)
    v = *c2da_postinc_2
    w = *q_0
    var c2da_postinc_3 : int? = q_0
    q_0 = unsafe(q_0 - 1)
    c2da_postinc_3
    x = *q_0
    c2da_rt_free(unsafe(unsafe(reinterpret<uint64>(unsafe(unsafe(reinterpret<uint8?>(p_0)))))))
    var c2da_fresh4 : int = int(0)
    if (int(d) == -int(128)) {
        c2da_fresh4 = int(1)
    }
    return int(c_0) * int(1000000) + c2da_fresh4 * int(100000) + v * int(1000) + w * int(10) + x
}
var cell : int

var calls : int

[export]
def get_cell() : int? {
    var c2da_postinc_4 : int = calls
    calls = int(calls + 1)
    c2da_postinc_4
    return unsafe(unsafe(reinterpret<int?>(unsafe(addr(cell)))))
}
[export]
def lvalue_once() : int {
    cell = int(1)
    calls = int(0)
    var c2da_fresh5 : int? = unsafe(unsafe(reinterpret<int?>(get_cell())))
    unsafe(*c2da_fresh5) = int(unsafe(*c2da_fresh5) + int(5))
    var c2da_fresh6 : int? = unsafe(unsafe(reinterpret<int?>(get_cell())))
    var c2da_postinc_5 : int = unsafe(*c2da_fresh6)
    unsafe(*c2da_fresh6) = int(unsafe(*c2da_fresh6) + 1)
    c2da_postinc_5
    return cell * int(10) + calls
}
[export]
def bool_conv() : int {
    var b : bool = false
    var z : bool = false
    var p_1 : int = int(0)
    var q_1 : bool = false
    var w_0 : bool = false
    b = int(5) != 0
    z = int(0) != 0
    p_1 = int(0)
    var c2da_fresh7 : int = int(0)
    if (unsafe(unsafe(addr(p_1)) != null)) {
        c2da_fresh7 = int(1)
    }
    q_1 = c2da_fresh7 != 0
    w_0 = 3.5lf != 0.0lf
    var c2da_fresh8 : int = int(0)
    if (b == true) {
        c2da_fresh8 = int(1)
    }
    var c2da_fresh9 : int = int(0)
    if (z == true) {
        c2da_fresh9 = int(1)
    }
    var c2da_fresh10 : int = int(0)
    if (q_1 == true) {
        c2da_fresh10 = int(1)
    }
    var c2da_fresh11 : int = int(0)
    if (w_0 == true) {
        c2da_fresh11 = int(1)
    }
    return c2da_fresh8 * int(8) + c2da_fresh9 * int(4) + c2da_fresh10 * int(2) + c2da_fresh11
}
[export]
def ptr_bytes() : int {
    var v_0 : uint = uint(0)
    var p_2 : uint8? = null
    v_0 = uint(0x11223344)
    p_2 = unsafe(unsafe(reinterpret<uint8?>(unsafe(addr(v_0)))))
    var c2da_fresh12 : int = int(0)
    if (int(unsafe(unsafe(unsafe(reinterpret<uint8?>(p_2)))[int(0)])) == int(68)) {
        c2da_fresh12 = int(1)
    }
    var c2da_fresh14 : int = 0
    if (c2da_fresh12 != 0) {
        var c2da_fresh13 : int = int(0)
        if (int(unsafe(unsafe(unsafe(reinterpret<uint8?>(p_2)))[int(3)])) == int(17)) {
            c2da_fresh13 = int(1)
        }
        if (c2da_fresh13 != 0) {
            c2da_fresh14 = 1
        }
    }
    return c2da_fresh14
}
[export]
def enum_big() : int {
    var c2da_fresh15 : int = int(0)
    if (HUGE > uint(int(0))) {
        c2da_fresh15 = int(1)
    }
    return c2da_fresh15
}
[export]
def sizeof_mul(var n : uint64) : int {
    var total : uint64 = uint64(0)
    total = uint64(4) * n
    var c2da_fresh16 : int = int(0)
    if (total == uint64(int(12))) {
        c2da_fresh16 = int(1)
    }
    return c2da_fresh16
}
[export]
def shifts() : int {
    var a_0 : int = int(0)
    var u_1 : uint = uint(0)
    a_0 = -int(16)
    u_1 = uint(0x80000000)
    var c2da_fresh17 : int = int(0)
    if (a_0 >> int(2) == -int(4)) {
        c2da_fresh17 = int(1)
    }
    var c2da_fresh19 : int = 0
    if (c2da_fresh17 != 0) {
        var c2da_fresh18 : int = int(0)
        if (u_1 >> uint(int(31)) == uint(int(1))) {
            c2da_fresh18 = int(1)
        }
        if (c2da_fresh18 != 0) {
            c2da_fresh19 = 1
        }
    }
    var c2da_fresh21 : int = 0
    if (c2da_fresh19 != 0) {
        var c2da_fresh20 : int = int(0)
        if (uint(0x1) << uint(int(31)) == uint(0x80000000)) {
            c2da_fresh20 = int(1)
        }
        if (c2da_fresh20 != 0) {
            c2da_fresh21 = 1
        }
    }
    var c2da_fresh23 : int = 0
    if (c2da_fresh21 != 0) {
        var c2da_fresh22 : int = int(0)
        if (a_0 * int(2) == -int(32)) {
            c2da_fresh22 = int(1)
        }
        if (c2da_fresh22 != 0) {
            c2da_fresh23 = 1
        }
    }
    return c2da_fresh23
}
[export]
def divmod() : int {
    var c2da_fresh24 : int = int(0)
    if (-int(7) / int(2) == -int(3)) {
        c2da_fresh24 = int(1)
    }
    var c2da_fresh26 : int = 0
    if (c2da_fresh24 != 0) {
        var c2da_fresh25 : int = int(0)
        if (-int(7) % int(2) == -int(1)) {
            c2da_fresh25 = int(1)
        }
        if (c2da_fresh25 != 0) {
            c2da_fresh26 = 1
        }
    }
    var c2da_fresh28 : int = 0
    if (c2da_fresh26 != 0) {
        var c2da_fresh27 : int = int(0)
        if (int(7) / -int(2) == -int(3)) {
            c2da_fresh27 = int(1)
        }
        if (c2da_fresh27 != 0) {
            c2da_fresh28 = 1
        }
    }
    var c2da_fresh30 : int = 0
    if (c2da_fresh28 != 0) {
        var c2da_fresh29 : int = int(0)
        if (int(7) % -int(2) == int(1)) {
            c2da_fresh29 = int(1)
        }
        if (c2da_fresh29 != 0) {
            c2da_fresh30 = 1
        }
    }
    var c2da_fresh32 : int = 0
    if (c2da_fresh30 != 0) {
        var c2da_fresh31 : int = int(0)
        if (uint(0x7) / uint(int(2)) == uint(int(3))) {
            c2da_fresh31 = int(1)
        }
        if (c2da_fresh31 != 0) {
            c2da_fresh32 = 1
        }
    }
    return c2da_fresh32
}
[export]
def mixed_cmp() : int {
    var i_2 : int = int(0)
    var u_2 : uint = uint(0)
    i_2 = -int(1)
    u_2 = uint(int(1))
    var c2da_fresh33 : int = int(0)
    if (uint(i_2) < u_2) {
        c2da_fresh33 = int(1)
    }
    var c2da_fresh34 : int = int(0)
    if (c2da_fresh33 == int(0)) {
        c2da_fresh34 = int(1)
    }
    return c2da_fresh34
}
[export]
def wide() : int {
    var big : int64 = int64(0)
    var ub : uint64 = uint64(0)
    big = int64(int(1))
    big = int64(big << int64(int(40)))
    ub = uint64(-int(1))
    var c2da_fresh35 : int = int(0)
    if (big == int64(0x10000000000uL)) {
        c2da_fresh35 = int(1)
    }
    var c2da_fresh37 : int = 0
    if (c2da_fresh35 != 0) {
        var c2da_fresh36 : int = int(0)
        if (ub >> uint64(int(63)) == uint64(int(1))) {
            c2da_fresh36 = int(1)
        }
        if (c2da_fresh36 != 0) {
            c2da_fresh37 = 1
        }
    }
    return c2da_fresh37
}
[export]
def integer_semantics_runtime() : int {
    if (int(short_wrap()) == 0) {
        goto label 16
    }
    if (int(ushort_wrap()) == 0) {
        goto label 15
    }
    if (int(short_assign()) == 0) {
        goto label 14
    }
    if (int(uchar_promotion()) == 0) {
        goto label 13
    }
    var c2da_fresh38 : int = int(0)
    if (int(narrowing()) != int(88)) {
        c2da_fresh38 = int(1)
    }
    if (c2da_fresh38 != 0) {
        goto label 12
    }
    var c2da_fresh39 : int = int(0)
    if (int(negative_index()) != int(21)) {
        c2da_fresh39 = int(1)
    }
    if (c2da_fresh39 != 0) {
        goto label 11
    }
    var c2da_fresh40 : int = int(0)
    if (int(compound()) != int(31444)) {
        c2da_fresh40 = int(1)
    }
    if (c2da_fresh40 != 0) {
        goto label 10
    }
    var c2da_fresh41 : int = int(0)
    if (int(incdec()) != int(105065)) {
        c2da_fresh41 = int(1)
    }
    if (c2da_fresh41 != 0) {
        goto label 9
    }
    var c2da_fresh42 : int = int(0)
    if (int(lvalue_once()) != int(72)) {
        c2da_fresh42 = int(1)
    }
    if (c2da_fresh42 != 0) {
        goto label 8
    }
    var c2da_fresh43 : int = int(0)
    if (int(bool_conv()) != int(11)) {
        c2da_fresh43 = int(1)
    }
    if (c2da_fresh43 != 0) {
        goto label 7
    }
    if (int(ptr_bytes()) == 0) {
        goto label 6
    }
    if (int(enum_big()) == 0) {
        goto label 5
    }
    if (int(sizeof_mul(uint64(int(3)))) == 0) {
        goto label 4
    }
    if (int(shifts()) == 0) {
        goto label 3
    }
    if (int(divmod()) == 0) {
        goto label 2
    }
    if (int(mixed_cmp()) == 0) {
        goto label 1
    }
    if (int(wide()) == 0) {
        goto label 0
    }
    return int(0)
    label 0:
    return int(17)
    label 1:
    return int(16)
    label 2:
    return int(15)
    label 3:
    return int(14)
    label 4:
    return int(13)
    label 5:
    return int(12)
    label 6:
    return int(11)
    label 7:
    return int(10)
    label 8:
    return int(9)
    label 9:
    return int(8)
    label 10:
    return int(7)
    label 11:
    return int(6)
    label 12:
    return int(5)
    label 13:
    return int(4)
    label 14:
    return int(3)
    label 15:
    return int(2)
    label 16:
    return int(1)
}
var HUGE : uint = uint(0x80000000)

