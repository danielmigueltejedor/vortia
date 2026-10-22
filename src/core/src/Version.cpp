#include "vortia/core/Version.hpp"

#include <charconv>

namespace vortia {

namespace {

/// Parses the leading unsigned integer of `token` ("2606", "0", "4-p1").
Expected<int> parseNumericPrefix(std::string_view token, std::string_view whole) {
    const char* begin = token.data();
    const char* end = begin + token.size();
    const char* digitEnd = begin;
    while (digitEnd != end && *digitEnd >= '0' && *digitEnd <= '9')
        ++digitEnd;
    if (digitEnd == begin)
        return Error::make(ErrorCode::ParseError, "version component is not numeric",
                           std::string(whole));
    int value = 0;
    auto [ptr, ec] = std::from_chars(begin, digitEnd, value);
    if (ec != std::errc())
        return Error::make(ErrorCode::ParseError, "version component out of range",
                           std::string(whole));
    return value;
}

} // namespace

Expected<Version> Version::parse(std::string_view text) {
    const auto first = text.find_first_not_of(" \t\r\n");
    if (first == std::string_view::npos)
        return Error::make(ErrorCode::ParseError, "empty version string");
    const auto last = text.find_last_not_of(" \t\r\n");
    std::string trimmed(text.substr(first, last - first + 1));

    Version v;
    v.raw = trimmed;

    std::string_view work = trimmed;
    if (!work.empty() && (work.front() == 'v' || work.front() == 'V'))
        work.remove_prefix(1);

    int fields[3] = {0, 0, 0};
    for (int field = 0; field < 3 && !work.empty(); ++field) {
        const auto dot = work.find('.');
        const std::string_view token =
            (dot == std::string_view::npos) ? work : work.substr(0, dot);
        auto number = parseNumericPrefix(token, trimmed);
        if (!number)
            return number.error();
        fields[field] = number.value();
        if (dot == std::string_view::npos)
            break;
        work.remove_prefix(dot + 1);
    }

    v.major = fields[0];
    v.minor = fields[1];
    v.patch = fields[2];
    return v;
}

} // namespace vortia
