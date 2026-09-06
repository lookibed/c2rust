//
//  AstExporter.hpp
//
//  Created by Alec Theriault on 10/4/18.
//

#ifndef AstExporter_hpp
#define AstExporter_hpp

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

using Outputs = std::unordered_map<std::string, std::vector<uint8_t>>;

Outputs process(int argc, const char *argv[], int *result,
                const std::string &trace_path = "");
void configure_exporter_debug(bool enabled);

#endif /* AstExporter_hpp */
