import shutil

import click
import gridfs
from bson.binary import Binary
from pymongo import MongoClient


@click.command()
@click.option('--mongo-url', default='mongodb://localhost:27017', help='MongoDB connection URL', show_default=True)
@click.option('--db-name', required=True, help='Database name')
@click.option('--collection-name', required=True, help='Collection name')
@click.option('--query-field', required=True, help='Field to query')
@click.option('--query-value', required=True, help='Value to query')
@click.option('--output-file', default='output.gzip', help='Output file name', show_default=True)
def write_gzip_blob(mongo_url, db_name, collection_name, query_field, query_value, output_file):
    """
    Retrieve a stored gzip log from MongoDB and write it to a file.

    Documents with storage=gridfs or gzip_blob_id are streamed from the
    default GridFS bucket. Legacy documents expose gzip_blob Binary.
    """
    client = MongoClient(mongo_url)
    try:
        db = client[db_name]
        collection = db[collection_name]
        query = {query_field: query_value}
        document = collection.find_one(query)

        if document:
            if document.get("storage") == "gridfs" or "gzip_blob_id" in document:
                if "gzip_blob_id" not in document:
                    click.echo("gzip_blob_id field is missing")
                else:
                    fs = gridfs.GridFS(db)
                    grid_out = fs.get(document["gzip_blob_id"])
                    with open(output_file, "wb") as file:
                        shutil.copyfileobj(grid_out, file)
                    click.echo(f"File written successfully: {output_file}")
            elif "gzip_blob" in document:
                gzip_blob = document["gzip_blob"]
                if isinstance(gzip_blob, (Binary, bytes)):
                    with open(output_file, "wb") as file:
                        file.write(gzip_blob)
                    click.echo(f"File written successfully: {output_file}")
                else:
                    click.echo("gzip_blob field is not of type Binary or bytes")
            else:
                click.echo("gzip_blob field is missing")
        else:
            click.echo("Document not found")
    except Exception as e:
        click.echo(f"Error: {e}")
    finally:
        client.close()


if __name__ == "__main__":
    write_gzip_blob()
