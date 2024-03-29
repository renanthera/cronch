from .request import Request

class SchemaIntrospection:
  schema_location = 'wcl/introspection_query.json'
  tree = {
    'name': '__schema'
  }
  paginator = {}
  cacheable = True

  def __str__( self ):
    with open( self.schema_location, 'r' ) as handle:
      data = handle.read()
    return data

schema_query = SchemaIntrospection()
schema_request_data = Request( schema_query ).data

# Headers of generated wcl type files:
objects = {
  'ENUM': [
    """from .primitives import GQL_ENUM

\"\"\"
ENUM TYPES
THIS FILE IS GENERATED BY `wcl/__main__.py`.
\"\"\""""
  ],
  'OBJECT': [
    """from .primitives import *
from .enums import *
from .scalars import *

\"\"\"
OBJECT TYPES
THIS FILE IS GENERATED BY `wcl/__main__.py`.
\"\"\""""
  ],
  'SCALAR': []
}

tab = '  '

def resolve_type( t ):
  kind = t[ 'kind' ]
  if kind in [ 'OBJECT', 'SCALAR', 'ENUM' ]:
    name = t[ 'name' ]
    return [ f'"GQL_{name}"' ]
  else:
    return [ f'"GQL_{kind}"', *resolve_type( t[ 'ofType' ] ) ]

def handle_enum( entry ):
  assert entry[ 'kind' ] == 'ENUM'
  longest_str = max( [ len( enum_value[ 'name' ] ) for enum_value in entry[ 'enumValues' ] ] )
  lines = [
    f'class GQL_{entry["name"]}( GQL_ENUM ):',
    f'{tab}enum_values = [',
    *[
      f'{tab}{tab}\'{name}\',{" "*(longest_str-len(name))} # {desc}'
      for enum_value in entry['enumValues']
      if ( name := enum_value['name'] )
      if ( desc := enum_value['description'] )
    ],
    f'{tab}]'
  ] # yapf: disable
  return '\n'.join( lines )

def handle_object( entry ):
  def format_type( t ):
    return f'[{",".join(resolve_type(t))}]'

  def format_field( field ):
    def base_filter( field ):
      key_filter = [ 'name', 'description' ]
      name = field[ 'name' ]
      description = field[ 'description' ].replace( '"', "'" ) if field[ 'description' ] is not None else ''
      return [
        f'{tab}"name": "{name}",',
        f'{tab}"descrption": "{description}",'
      ] # yapf: disable
      return [
        f'{tab}"{key}": "{value}",'
        for key, value in field.items()
        if key in key_filter
      ] # yapf: disable

    def format_arg( arg ):
      return [
        '{',
        *[
          f'{line}'
          for line in base_filter( arg )
        ],
        f'{tab}"type": {format_type(arg["type"])},',
        '},'
      ] # yapf: disable

    def format_args( args ):
      if args:
        return [
          '"args": [',
          *[
            f'{tab}{line}'
            for arg in args
            if ( arg_lines := format_arg( arg ) )
            for line in arg_lines
          ],
          ']'
        ] # yapf: disable
      return []

    return [
      '{',
      *base_filter( field ),
      f'{tab}"type": {format_type(field["type"])},',
      *[
        f'{tab}{line}'
        for line in format_args( field[ 'args' ] )
      ],
      '},'
    ] # yapf: disable

  assert entry[ 'kind' ] == 'OBJECT'
  descr = [
    f'{tab}"""',
    f'{tab}{entry["description"]}',
    f'{tab}"""'
  ] if entry[ 'description' ] else []
  lines = [
    f'class GQL_{entry["name"]}( GQL_OBJECT ):',
    *descr,
    f'{tab}fields = [',
    *[
      f'{tab}{tab}{line}'
      for field in entry[ 'fields' ]
      if ( formatted_field := format_field( field ) )
      for line in formatted_field
    ],
    f'{tab}]'
  ] # yapf: disable
  return '\n'.join( lines )

for k in schema_request_data[ 'types' ]:
  kind = k[ 'kind' ]
  if kind in objects.keys():
    entry = {
      'name': k[ 'name' ],
      'data': k,
      'value': None
    }
    match kind:
      case 'ENUM':
        entry.update( {
          'string': handle_enum( k )
        } )
      case 'OBJECT':
        entry.update( {
          'string': handle_object( k )
        } )
      case 'SCALAR':
        pass
      case _:
        pass
    objects[ kind ].append( entry ) # pyright: ignore

def join_values( values ):
  for value in values:
    if isinstance( value, dict ):
      yield ''.join( value[ 'string' ] )
    else:
      yield ''.join( value )

for kind, values in objects.items():
  if kind in [ 'OBJECT', 'ENUM' ]:
    with open( f'wcl/types/{kind.lower()}s.py', 'w' ) as handle:
      handle.write( '\n\n'.join( join_values( values ) ) )
