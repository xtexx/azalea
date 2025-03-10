import lib.code.version
import lib.code.entity
import lib.code.utils
import lib.download
import lib.extract

version_id = lib.code.version.get_version_id()

mappings = lib.download.get_mappings_for_version(version_id)
burger_data = lib.extract.get_burger_data_for_version(version_id)

burger_entities_data = burger_data[0]['entities']

lib.code.entity.generate_entity_metadata(burger_entities_data, mappings)
lib.code.entity.generate_entity_dimensions(burger_entities_data)

lib.code.utils.fmt()

print('Done!')
